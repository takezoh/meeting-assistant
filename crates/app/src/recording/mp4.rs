//! Rewriting a finished MP4 without decoding anything and without moving any of its timestamps: adding an audio track
//! to a fragmented one, and writing metadata into any.
//!
//! A browser records a tab as a fragmented MP4: a `moov` that describes the tracks, then `moof`/`mdat` pairs, each
//! fragment saying when its samples start (`tfdt`) and where its data is, relative to the fragment itself. Such a file
//! takes another track by rewriting only the `moov` (one more `trak`, one more `trex`) and appending fragments for the
//! new track; every sample and timestamp after the `moov` is copied as it is, so the recording keeps exactly the timing
//! it was written with. Only offsets counted from the start of the file, which some writers use instead, move with the
//! `moov`.
//!
//! The added track comes from the app's own microphone recording: an ordinary MP4 (sample tables in its `moov`), as
//! AVAssetWriter finishes it on macOS, or a fragmented one, as Media Foundation writes it on Windows. Media Foundation's
//! fragments do not say when they start (no `tfdt`) and address their data from the start of the file.
//!
//! It also writes metadata into a finished recording of any kind ([`with_metadata`]).
//!
//! This module reads through [`ReadAt`] and returns what to write as a list of [`Piece`]s; it touches no file.

use std::collections::HashMap;
use std::io;
use std::time::Duration;

use anyhow::Context;

use super::merge::Placement;

/// Random access to a file's bytes.
pub trait ReadAt {
    fn len(&self) -> io::Result<u64>;
    /// Fills `buffer` from `offset`; fails if the file ends first.
    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<()>;
}

impl ReadAt for Vec<u8> {
    fn len(&self) -> io::Result<u64> {
        Ok(Vec::len(self) as u64)
    }

    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<()> {
        let start = usize::try_from(offset).map_err(io::Error::other)?;
        let source = self
            .get(start..start + buffer.len())
            .ok_or(io::ErrorKind::UnexpectedEof)?;
        buffer.copy_from_slice(source);
        Ok(())
    }
}

/// One part of the output, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Bytes(Vec<u8>),
    /// A range of the recording, copied as it is.
    Recording {
        offset: u64,
        length: u64,
    },
    /// A range of the microphone recording, copied as it is.
    Microphone {
        offset: u64,
        length: u64,
    },
}

/// How many of the added track's samples go in one fragment: about two seconds of AAC.
const SAMPLES_PER_FRAGMENT: usize = 94;

/// The pieces of `recording` with the audio track of `microphone` added as its last track, placed by `placement` and cut
/// at the end of `recording`.
///
/// The added track's fragments follow the recording's, before a fragment index at the end, which stays last. When the
/// recording's fragments address their data from the start of the file, their offsets and the index move by as much as
/// the `moov` grew, as in [`with_metadata`].
pub fn add_audio_track(
    recording: &dyn ReadAt,
    microphone: &dyn ReadAt,
    placement: Placement,
) -> anyhow::Result<Vec<Piece>> {
    let recording_boxes = top_level(recording).context("cannot read the recording")?;
    let moov = only_movie(&recording_boxes)?;
    let moov_bytes = read_body(recording, moov)?;
    let movie = Movie::parse(&moov_bytes)?;
    let mut end = Duration::ZERO;
    let mut next_sequence = 1;
    let mut next_time = HashMap::new();
    for moof in recording_boxes.iter().filter(|b| &b.kind == b"moof") {
        let body = read_body(recording, moof)?;
        next_sequence = next_sequence.max(u32_at(child(&body, b"mfhd")?, 4)? + 1);
        for (track, sample) in fragment_samples(moof.start, &body, &movie.defaults, &mut next_time)? {
            let ticks = u128::from(sample.time + u64::from(sample.duration));
            let timescale = u128::from(movie.timescale(track)?);
            end = end.max(Duration::from_nanos(u64::try_from(ticks * 1_000_000_000 / timescale)?));
        }
    }
    anyhow::ensure!(end > Duration::ZERO, "the recording has no samples");

    let source = Source::parse(microphone).context("cannot read the microphone recording")?;
    let track_id = movie.next_track_id();
    let samples = source.placed(placement, end);
    anyhow::ensure!(
        !samples.is_empty(),
        "the microphone recording does not overlap the recording"
    );

    let new_moov = movie.with_track(&moov_bytes, &source, track_id)?;
    let grown = i64::try_from(new_moov.len())? - i64::try_from(moov.size)?;
    let shift = moved_past(moov.start + moov.size, grown);
    let mut pieces = pieces_with_movie(recording, &recording_boxes, new_moov, grown, &shift)?;
    let mut added = Vec::new();
    for (index, chunk) in samples.chunks(SAMPLES_PER_FRAGMENT).enumerate() {
        fragment(&mut added, chunk, track_id, next_sequence + index as u32);
    }
    // Readers find a fragment index from the end of the file.
    let at = if recording_boxes.last().is_some_and(|b| &b.kind == b"mfra") {
        pieces.len() - 1
    } else {
        pieces.len()
    };
    pieces.splice(at..at, added);
    Ok(merge_ranges(pieces))
}

/// The one `moov` among `boxes`.
fn only_movie(boxes: &[BoxRef]) -> anyhow::Result<&BoxRef> {
    let mut movies = boxes.iter().filter(|b| &b.kind == b"moov");
    let moov = movies.next().context("the recording has no movie header")?;
    anyhow::ensure!(movies.next().is_none(), "the recording has more than one movie header");
    Ok(moov)
}

/// Where an offset in a file goes when its `moov`, which ended at `end`, grows by `grown` bytes: an offset past it moves
/// with it.
fn moved_past(end: u64, grown: i64) -> impl Fn(u64) -> anyhow::Result<u64> {
    move |offset| {
        if offset < end {
            return Ok(offset);
        }
        offset
            .checked_add_signed(grown)
            .context("an offset in the recording is out of range")
    }
}

/// One piece for each of the `boxes` of `recording`, whose `moov` becomes `moov` (a whole box), `grown` bytes larger.
/// Fragments' explicit base data offsets (`tfhd`) and the fragment index (`tfra`) go through `shift`; every other box
/// is copied as it is.
fn pieces_with_movie(
    recording: &dyn ReadAt,
    boxes: &[BoxRef],
    moov: Vec<u8>,
    grown: i64,
    shift: &dyn Fn(u64) -> anyhow::Result<u64>,
) -> anyhow::Result<Vec<Piece>> {
    let mut moov = Some(moov);
    let mut pieces = Vec::with_capacity(boxes.len());
    for b in boxes {
        let rewritten = match &b.kind {
            b"moov" => moov.take(),
            b"moof" if grown != 0 => {
                with_base_offsets(&read_body(recording, b)?, shift)?.map(|body| make_box(b"moof", &body))
            }
            b"mfra" if grown != 0 => Some(make_box(
                b"mfra",
                &with_fragment_index(&read_body(recording, b)?, shift)?,
            )),
            _ => None,
        };
        pieces.push(rewritten.map_or(
            Piece::Recording {
                offset: b.start,
                length: b.size,
            },
            Piece::Bytes,
        ));
    }
    Ok(pieces)
}

/// The pieces of `recording` with `entries` written into its `moov` as QuickTime metadata: a `meta` box right in the
/// `moov`, with an `mdta` handler, the entries' keys and their values as UTF-8, which ffmpeg and AVFoundation both read
/// by key. A `meta` box with an `mdta` handler already in the `moov` is replaced.
///
/// Every other byte is copied. When the `moov` comes before media, the offsets that point past it move by as much as it
/// grew: chunk offsets (`stco`, `co64`), a fragment's explicit base data offset (`tfhd`) and the fragment index
/// (`tfra`). Fragments whose data is addressed from the fragment itself, as browsers and the platforms write them, need
/// nothing; neither does a file whose `moov` comes last.
pub fn with_metadata(recording: &dyn ReadAt, entries: &[(String, String)]) -> anyhow::Result<Vec<Piece>> {
    let boxes = top_level(recording).context("cannot read the recording")?;
    let moov = only_movie(&boxes)?;
    let moov_body = read_body(recording, moov)?;

    let mut body = Vec::with_capacity(moov_body.len() + 512);
    for (kind, whole, child_body) in children(&moov_body)? {
        if &kind == b"meta" && is_metadata_of_keys(child_body) {
            continue;
        }
        match &kind {
            // Rebuilt with the headers `make_box` writes, so that the size is final before any offset moves.
            b"trak" => body.extend_from_slice(&make_box(
                b"trak",
                &with_chunk_offsets(child_body, &|offset| Ok(offset))?,
            )),
            _ => body.extend_from_slice(whole),
        }
    }
    body.extend_from_slice(&metadata_box(entries));

    let grown = i64::try_from(body.len() + 8)? - i64::try_from(moov.size)?;
    let shift = moved_past(moov.start + moov.size, grown);
    let body = with_chunk_offsets(&body, &shift)?;
    let pieces = pieces_with_movie(recording, &boxes, make_box(b"moov", &body), grown, &shift)?;
    Ok(merge_ranges(pieces))
}

/// Whether a `meta` body holds QuickTime metadata keys: it has an `hdlr` of type `mdta`, with or without the version
/// and flags an ISO `meta` box begins with.
fn is_metadata_of_keys(meta: &[u8]) -> bool {
    [meta, meta.get(4..).unwrap_or_default()].into_iter().any(|body| {
        children(body).is_ok_and(|found| {
            found
                .iter()
                .any(|(kind, _, hdlr)| kind == b"hdlr" && hdlr.get(8..12) == Some(b"mdta".as_slice()))
        })
    })
}

/// A `meta` box of QuickTime metadata keys holding `entries`. It has no version and flags, as QuickTime has it;
/// AVFoundation reads nothing from one that does.
fn metadata_box(entries: &[(String, String)]) -> Vec<u8> {
    // pre_defined, the handler type, three reserved words and an empty name.
    let hdlr = [&0u32.to_be_bytes()[..], b"mdta", &[0; 13]].concat();
    let mut keys = (entries.len() as u32).to_be_bytes().to_vec();
    let mut items = Vec::new();
    for (index, (key, value)) in entries.iter().enumerate() {
        keys.extend_from_slice(&make_box(b"mdta", key.as_bytes()));
        // Type 1 (UTF-8), then the default locale.
        let data = [&1u32.to_be_bytes()[..], &0u32.to_be_bytes(), value.as_bytes()].concat();
        items.extend_from_slice(&make_box(&(index as u32 + 1).to_be_bytes(), &make_box(b"data", &data)));
    }
    let body = [
        full_box(b"hdlr", 0, 0, &hdlr),
        full_box(b"keys", 0, 0, &keys),
        make_box(b"ilst", &items),
    ]
    .concat();
    make_box(b"meta", &body)
}

/// `container` with every chunk offset in its tracks passed through `shift`.
fn with_chunk_offsets(container: &[u8], shift: &dyn Fn(u64) -> anyhow::Result<u64>) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(container.len());
    for (kind, whole, body) in children(container)? {
        match &kind {
            b"trak" | b"mdia" | b"minf" | b"stbl" => {
                out.extend_from_slice(&make_box(&kind, &with_chunk_offsets(body, shift)?));
            }
            b"stco" | b"co64" => {
                let wide = &kind == b"co64";
                let width = if wide { 8 } else { 4 };
                let mut patched = body.to_vec();
                let count = u32_at(&patched, 4)? as usize;
                for index in 0..count {
                    let at = 8 + index * width;
                    let offset = if wide {
                        shift(u64_at(&patched, at)?)?
                    } else {
                        shift(u64::from(u32_at(&patched, at)?))?
                    };
                    if wide {
                        patched[at..at + 8].copy_from_slice(&offset.to_be_bytes());
                    } else {
                        let offset = u32::try_from(offset).context("a chunk offset no longer fits in 32 bits")?;
                        patched[at..at + 4].copy_from_slice(&offset.to_be_bytes());
                    }
                }
                out.extend_from_slice(&make_box(&kind, &patched));
            }
            _ => out.extend_from_slice(whole),
        }
    }
    Ok(out)
}

/// A `moof` body with each explicit base data offset passed through `shift`, or `None` if it has none.
fn with_base_offsets(moof: &[u8], shift: &dyn Fn(u64) -> anyhow::Result<u64>) -> anyhow::Result<Option<Vec<u8>>> {
    let mut changed = false;
    let mut out = Vec::with_capacity(moof.len());
    for (kind, whole, traf) in children(moof)? {
        if &kind != b"traf" {
            out.extend_from_slice(whole);
            continue;
        }
        let mut body = Vec::with_capacity(traf.len());
        for (kind, whole, tfhd) in children(traf)? {
            // base-data-offset-present: the offset follows the version, flags and track ID.
            if &kind == b"tfhd" && u32_at(tfhd, 0)? & 1 != 0 {
                let mut patched = tfhd.to_vec();
                let offset = shift(u64_at(&patched, 8)?)?;
                patched[8..16].copy_from_slice(&offset.to_be_bytes());
                body.extend_from_slice(&make_box(b"tfhd", &patched));
                changed = true;
            } else {
                body.extend_from_slice(whole);
            }
        }
        out.extend_from_slice(&make_box(b"traf", &body));
    }
    Ok(changed.then_some(out))
}

/// An `mfra` body with the fragment offsets in each `tfra` passed through `shift`.
fn with_fragment_index(mfra: &[u8], shift: &dyn Fn(u64) -> anyhow::Result<u64>) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(mfra.len());
    for (kind, whole, tfra) in children(mfra)? {
        if &kind != b"tfra" {
            out.extend_from_slice(whole);
            continue;
        }
        let mut patched = tfra.to_vec();
        let wide = patched.first() == Some(&1);
        let lengths = u32_at(&patched, 8)?;
        // The sizes of traf_number, trun_number and sample_number, each one byte more than its two bits say.
        let numbers: usize = [4, 2, 0].iter().map(|bits| ((lengths >> bits) & 3) as usize + 1).sum();
        let count = u32_at(&patched, 12)? as usize;
        let entry = if wide { 16 } else { 8 } + numbers;
        for index in 0..count {
            let at = 16 + index * entry + if wide { 8 } else { 4 };
            if wide {
                let offset = shift(u64_at(&patched, at)?)?;
                patched[at..at + 8].copy_from_slice(&offset.to_be_bytes());
            } else {
                let offset = u32::try_from(shift(u64::from(u32_at(&patched, at)?))?)
                    .context("a fragment offset no longer fits in 32 bits")?;
                patched[at..at + 4].copy_from_slice(&offset.to_be_bytes());
            }
        }
        out.extend_from_slice(&make_box(b"tfra", &patched));
    }
    Ok(out)
}

/// A box in a file: its type, where it starts, the size of its header and its whole size.
#[derive(Debug, Clone)]
struct BoxRef {
    kind: [u8; 4],
    start: u64,
    header: u64,
    size: u64,
}

/// The boxes at the top of a file, read from their headers.
fn top_level(file: &dyn ReadAt) -> anyhow::Result<Vec<BoxRef>> {
    let length = file.len()?;
    let mut boxes = Vec::new();
    let mut position = 0;
    while position + 8 <= length {
        let mut header = [0u8; 16];
        file.read_at(position, &mut header[..8])?;
        let size32 = u32::from_be_bytes(header[..4].try_into()?);
        let kind: [u8; 4] = header[4..8].try_into()?;
        let (header_size, size) = match size32 {
            1 => {
                file.read_at(position + 8, &mut header[8..16])?;
                (16, u64::from_be_bytes(header[8..16].try_into()?))
            }
            0 => (8, length - position),
            size => (8, u64::from(size)),
        };
        anyhow::ensure!(
            size >= header_size && position + size <= length,
            "a box runs past the end of the file"
        );
        boxes.push(BoxRef {
            kind,
            start: position,
            header: header_size,
            size,
        });
        position += size;
    }
    Ok(boxes)
}

fn read_body(file: &dyn ReadAt, b: &BoxRef) -> anyhow::Result<Vec<u8>> {
    let length = usize::try_from(b.size - b.header)?;
    anyhow::ensure!(length <= 64 << 20, "a header box is implausibly large");
    let mut body = vec![0u8; length];
    file.read_at(b.start + b.header, &mut body)?;
    Ok(body)
}

/// A box inside another: its type, the whole box and its body.
type Child<'a> = ([u8; 4], &'a [u8], &'a [u8]);

/// The boxes directly inside `body`.
fn children(body: &[u8]) -> anyhow::Result<Vec<Child<'_>>> {
    let mut found = Vec::new();
    let mut rest = body;
    while rest.len() >= 8 {
        let size32 = u32::from_be_bytes(rest[..4].try_into()?) as usize;
        let kind: [u8; 4] = rest[4..8].try_into()?;
        let (header, size) = match size32 {
            1 => (
                16,
                usize::try_from(u64::from_be_bytes(
                    rest.get(8..16).context("truncated box")?.try_into()?,
                ))?,
            ),
            0 => (8, rest.len()),
            size => (8, size),
        };
        anyhow::ensure!(size >= header && size <= rest.len(), "a box runs past its container");
        found.push((kind, &rest[..size], &rest[header..size]));
        rest = &rest[size..];
    }
    Ok(found)
}

fn child<'a>(body: &'a [u8], kind: &[u8; 4]) -> anyhow::Result<&'a [u8]> {
    children(body)?
        .into_iter()
        .find(|(k, _, _)| k == kind)
        .map(|(_, _, body)| body)
        .with_context(|| format!("no {} box", String::from_utf8_lossy(kind)))
}

fn u32_at(bytes: &[u8], offset: usize) -> anyhow::Result<u32> {
    Ok(u32::from_be_bytes(
        bytes.get(offset..offset + 4).context("truncated box")?.try_into()?,
    ))
}

fn u64_at(bytes: &[u8], offset: usize) -> anyhow::Result<u64> {
    Ok(u64::from_be_bytes(
        bytes.get(offset..offset + 8).context("truncated box")?.try_into()?,
    ))
}

fn make_box(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend_from_slice(&(body.len() as u32 + 8).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    out
}

/// A full box's version and flags followed by `fields`.
fn full_box(kind: &[u8; 4], version: u8, flags: u32, fields: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(fields.len() + 4);
    body.extend_from_slice(&((u32::from(version) << 24) | (flags & 0xff_ffff)).to_be_bytes());
    body.extend_from_slice(fields);
    make_box(kind, &body)
}

/// The track ID and timescale in `tkhd` and `mdhd` bodies.
fn track_id(tkhd: &[u8]) -> anyhow::Result<u32> {
    u32_at(tkhd, if tkhd.first() == Some(&1) { 20 } else { 12 })
}

fn timescale(mdhd: &[u8]) -> anyhow::Result<u32> {
    let timescale = u32_at(mdhd, if mdhd.first() == Some(&1) { 20 } else { 12 })?;
    anyhow::ensure!(timescale > 0, "a track has no timescale");
    Ok(timescale)
}

/// What the recording's `moov` says that the merge needs.
struct Movie {
    /// (track ID, timescale) of each track.
    tracks: Vec<(u32, u32)>,
    /// What each track's samples in fragments are when a fragment does not say.
    defaults: Vec<(u32, Defaults)>,
}

impl Movie {
    fn parse(moov: &[u8]) -> anyhow::Result<Self> {
        let mut tracks = Vec::new();
        for (kind, _, trak) in children(moov)? {
            if &kind != b"trak" {
                continue;
            }
            let mdia = child(trak, b"mdia")?;
            let stco = child(child(child(mdia, b"minf")?, b"stbl")?, b"stco").ok();
            anyhow::ensure!(
                stco.is_none_or(|stco| u32_at(stco, 4).is_ok_and(|count| count == 0)),
                "the recording is not a fragmented MP4"
            );
            tracks.push((track_id(child(trak, b"tkhd")?)?, timescale(child(mdia, b"mdhd")?)?));
        }
        let mvex = child(moov, b"mvex").context("the recording is not a fragmented MP4")?;
        let defaults = Defaults::of_tracks(mvex)?;
        anyhow::ensure!(!tracks.is_empty(), "the recording has no tracks");
        Ok(Self { tracks, defaults })
    }

    fn next_track_id(&self) -> u32 {
        self.tracks.iter().map(|(id, _)| *id).max().unwrap_or(0) + 1
    }

    /// The recording's `moov` with `source`'s track added as `track_id`.
    fn with_track(&self, moov: &[u8], source: &Source, track_id: u32) -> anyhow::Result<Vec<u8>> {
        let mut body = Vec::with_capacity(moov.len() + 1024);
        let mut trak_written = false;
        for (kind, whole, child_body) in children(moov)? {
            match &kind {
                b"mvhd" => {
                    // next_track_ID is the last field.
                    let mut patched = child_body.to_vec();
                    let at = patched.len().checked_sub(4).context("truncated movie header")?;
                    patched[at..].copy_from_slice(&(track_id + 1).to_be_bytes());
                    body.extend_from_slice(&make_box(b"mvhd", &patched));
                }
                b"mvex" => {
                    if !trak_written {
                        body.extend_from_slice(&source.trak(track_id)?);
                        trak_written = true;
                    }
                    let mut mvex = child_body.to_vec();
                    // trex: track ID, sample description 1, no default duration, size or flags.
                    let mut fields = Vec::new();
                    for value in [track_id, 1, 0, 0, 0] {
                        fields.extend_from_slice(&value.to_be_bytes());
                    }
                    mvex.extend_from_slice(&full_box(b"trex", 0, 0, &fields));
                    body.extend_from_slice(&make_box(b"mvex", &mvex));
                }
                _ => body.extend_from_slice(whole),
            }
        }
        anyhow::ensure!(trak_written, "the recording is not a fragmented MP4");
        Ok(make_box(b"moov", &body))
    }

    fn timescale(&self, track: u32) -> anyhow::Result<u32> {
        self.tracks
            .iter()
            .find(|(id, _)| *id == track)
            .map(|(_, timescale)| *timescale)
            .context("a fragment names an unknown track")
    }
}

/// What a track's samples in fragments are when neither the fragment nor its run says: its `trex`.
#[derive(Debug, Clone, Copy, Default)]
struct Defaults {
    duration: u32,
    size: u32,
}

impl Defaults {
    /// (track ID, defaults) of each `trex` in `mvex`.
    fn of_tracks(mvex: &[u8]) -> anyhow::Result<Vec<(u32, Self)>> {
        children(mvex)?
            .into_iter()
            .filter(|(kind, _, _)| kind == b"trex")
            .map(|(_, _, trex)| {
                let defaults = Self {
                    duration: u32_at(trex, 12)?,
                    size: u32_at(trex, 16)?,
                };
                Ok((u32_at(trex, 4)?, defaults))
            })
            .collect()
    }
}

/// Every sample in the `moof` that starts at `moof_start`, as (track ID, sample), with its place in the file and its
/// decode time in its track's timescale.
///
/// A track fragment starts at its `tfdt`, or else where the track's last fragment ended, which `next_time` keeps by
/// track ID (from 0). Its data starts at its explicit base offset, at the `moof` when it says so or is the first track
/// fragment, or else where the data of the track fragment before it ended; a run without an offset follows the run
/// before it.
fn fragment_samples(
    moof_start: u64,
    moof: &[u8],
    defaults: &[(u32, Defaults)],
    next_time: &mut HashMap<u32, u64>,
) -> anyhow::Result<Vec<(u32, Sample)>> {
    let mut samples = Vec::new();
    let mut data_end = moof_start;
    for (kind, _, traf) in children(moof)? {
        if &kind != b"traf" {
            continue;
        }
        let tfhd = child(traf, b"tfhd")?;
        let flags = u32_at(tfhd, 0)? & 0xff_ffff;
        let track = u32_at(tfhd, 4)?;
        let track_defaults = defaults
            .iter()
            .find(|(id, _)| *id == track)
            .map_or_else(Defaults::default, |(_, defaults)| *defaults);
        // Optional fields follow the track ID in this order.
        let mut at = 8;
        let mut field = |flag: u32, size: usize| -> anyhow::Result<Option<u64>> {
            if flags & flag == 0 {
                return Ok(None);
            }
            let value = if size == 8 {
                u64_at(tfhd, at)?
            } else {
                u64::from(u32_at(tfhd, at)?)
            };
            at += size;
            Ok(Some(value))
        };
        let explicit_base = field(0x01, 8)?;
        field(0x02, 4)?;
        let default_duration = field(0x08, 4)?.map_or(track_defaults.duration, |value| value as u32);
        let default_size = field(0x10, 4)?.map_or(track_defaults.size, |value| value as u32);
        let base = explicit_base.unwrap_or(if flags & 0x02_0000 != 0 { moof_start } else { data_end });

        let mut time = match child(traf, b"tfdt") {
            Ok(tfdt) if tfdt.first() == Some(&1) => u64_at(tfdt, 4)?,
            Ok(tfdt) => u64::from(u32_at(tfdt, 4)?),
            Err(_) => next_time.get(&track).copied().unwrap_or(0),
        };
        let mut position = base;
        for (kind, _, trun) in children(traf)? {
            if &kind != b"trun" {
                continue;
            }
            let run_flags = u32_at(trun, 0)? & 0xff_ffff;
            let count = u32_at(trun, 4)? as usize;
            let mut at = 8;
            if run_flags & 0x01 != 0 {
                let offset = i64::from(u32_at(trun, at)? as i32);
                position = base
                    .checked_add_signed(offset)
                    .context("a fragment's data lies before the file")?;
                at += 4;
            }
            if run_flags & 0x04 != 0 {
                at += 4;
            }
            for _ in 0..count {
                let mut per_sample = |flag: u32, default: u32| -> anyhow::Result<u32> {
                    if run_flags & flag == 0 {
                        return Ok(default);
                    }
                    let value = u32_at(trun, at)?;
                    at += 4;
                    Ok(value)
                };
                let duration = per_sample(0x100, default_duration)?;
                let size = per_sample(0x200, default_size)?;
                // Flags and composition offsets are not needed.
                per_sample(0x400, 0)?;
                per_sample(0x800, 0)?;
                samples.push((
                    track,
                    Sample {
                        offset: position,
                        size,
                        duration,
                        time,
                    },
                ));
                position += u64::from(size);
                time += u64::from(duration);
            }
        }
        data_end = position;
        next_time.insert(track, time);
    }
    Ok(samples)
}

/// One sample of the added track, placed on the recording's timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sample {
    offset: u64,
    size: u32,
    duration: u32,
    /// When it is decoded, in the track's timescale, on the recording's timeline.
    time: u64,
}

/// The audio track of an MP4, ordinary or fragmented.
struct Source {
    tkhd: Vec<u8>,
    mdhd: Vec<u8>,
    hdlr: Vec<u8>,
    smhd: Vec<u8>,
    dinf: Vec<u8>,
    stsd: Vec<u8>,
    timescale: u32,
    /// The media time the track's presentation starts at (the encoder's priming), from its edit list.
    media_start: u64,
    samples: Vec<Sample>,
}

impl Source {
    fn parse(file: &dyn ReadAt) -> anyhow::Result<Self> {
        let boxes = top_level(file)?;
        let moov = boxes
            .iter()
            .find(|b| &b.kind == b"moov")
            .context("it has no movie header")?;
        let moov = read_body(file, moov)?;
        let trak = children(&moov)?
            .into_iter()
            .filter(|(kind, _, _)| kind == b"trak")
            .map(|(_, _, trak)| trak)
            .find(|trak| {
                child(trak, b"mdia")
                    .and_then(|mdia| child(mdia, b"hdlr"))
                    .is_ok_and(|hdlr| hdlr.get(8..12) == Some(b"soun"))
            })
            .context("it has no audio track")?;
        let mdia = child(trak, b"mdia")?;
        let minf = child(mdia, b"minf")?;
        let stbl = child(minf, b"stbl")?;
        let whole = |body: &[u8], kind: &[u8; 4]| -> anyhow::Result<Vec<u8>> {
            children(body)?
                .into_iter()
                .find(|(k, _, _)| k == kind)
                .map(|(_, whole, _)| whole.to_vec())
                .with_context(|| format!("no {} box", String::from_utf8_lossy(kind)))
        };
        let media_start = match child(trak, b"edts").and_then(|edts| child(edts, b"elst")) {
            Ok(elst) => first_media_time(elst)?,
            Err(_) => 0,
        };
        // Samples described in the sample table come first; a fragmented file's follow in its fragments.
        let track = track_id(child(trak, b"tkhd")?)?;
        let mut samples = sample_table(stbl)?;
        if let Ok(mvex) = child(&moov, b"mvex") {
            let defaults = Defaults::of_tracks(mvex)?;
            let start = samples.last().map_or(0, |last| last.time + u64::from(last.duration));
            let mut next_time = HashMap::from([(track, start)]);
            for moof in boxes.iter().filter(|b| &b.kind == b"moof") {
                let found = fragment_samples(moof.start, &read_body(file, moof)?, &defaults, &mut next_time)?;
                samples.extend(
                    found
                        .into_iter()
                        .filter(|(id, _)| *id == track)
                        .map(|(_, sample)| sample),
                );
            }
        }
        let length = file.len()?;
        anyhow::ensure!(
            samples
                .iter()
                .all(|sample| sample.offset.saturating_add(u64::from(sample.size)) <= length),
            "a sample lies past the end of the file"
        );
        Ok(Self {
            tkhd: child(trak, b"tkhd")?.to_vec(),
            mdhd: child(mdia, b"mdhd")?.to_vec(),
            hdlr: whole(mdia, b"hdlr")?,
            smhd: whole(minf, b"smhd")?,
            dinf: whole(minf, b"dinf")?,
            stsd: whole(stbl, b"stsd")?,
            timescale: timescale(child(mdia, b"mdhd")?)?,
            media_start,
            samples,
        })
    }

    /// The samples that fall on the recording's timeline, with their times there: the encoder's priming and
    /// `placement.skip` are dropped, `placement.delay` is added, and nothing at or after `end` is kept.
    fn placed(&self, placement: Placement, end: Duration) -> Vec<Sample> {
        // To the nearest tick: a duration in nanoseconds rarely falls on one exactly.
        let ticks = |duration: Duration| {
            u64::try_from((duration.as_nanos() * u128::from(self.timescale) + 500_000_000) / 1_000_000_000)
                .unwrap_or(u64::MAX)
        };
        let dropped = self.media_start + ticks(placement.skip);
        let (delay, end) = (ticks(placement.delay), ticks(end));
        self.samples
            .iter()
            .filter(|sample| sample.time >= dropped)
            .map(|sample| Sample {
                time: sample.time - dropped + delay,
                ..*sample
            })
            .take_while(|sample| sample.time < end)
            .collect()
    }

    /// A `trak` for this track as `track_id`, whose samples are all in fragments.
    fn trak(&self, track_id: u32) -> anyhow::Result<Vec<u8>> {
        let mut tkhd = self.tkhd.clone();
        let version1 = tkhd.first() == Some(&1);
        let (id_at, duration_at, duration_size, group_at) = if version1 { (20, 28, 8, 46) } else { (12, 20, 4, 34) };
        anyhow::ensure!(tkhd.len() >= group_at + 2, "truncated track header");
        tkhd[id_at..id_at + 4].copy_from_slice(&track_id.to_be_bytes());
        tkhd[duration_at..duration_at + duration_size].fill(0);
        // Not an alternative to the recording's audio: both are meant to be used.
        tkhd[group_at..group_at + 2].fill(0);
        let mut mdhd = self.mdhd.clone();
        let (duration_at, duration_size) = if mdhd.first() == Some(&1) { (24, 8) } else { (16, 4) };
        anyhow::ensure!(mdhd.len() >= duration_at + duration_size, "truncated media header");
        mdhd[duration_at..duration_at + duration_size].fill(0);

        let empty = |kind: &[u8; 4], fields: usize| full_box(kind, 0, 0, &vec![0u8; fields]);
        let stbl = [
            self.stsd.clone(),
            empty(b"stts", 4),
            empty(b"stsc", 4),
            empty(b"stsz", 8),
            empty(b"stco", 4),
        ]
        .concat();
        let minf = [self.smhd.clone(), self.dinf.clone(), make_box(b"stbl", &stbl)].concat();
        let mdia = [make_box(b"mdhd", &mdhd), self.hdlr.clone(), make_box(b"minf", &minf)].concat();
        Ok(make_box(
            b"trak",
            &[make_box(b"tkhd", &tkhd), make_box(b"mdia", &mdia)].concat(),
        ))
    }
}

/// The media time of the first edit that is not empty.
fn first_media_time(elst: &[u8]) -> anyhow::Result<u64> {
    let version1 = elst.first() == Some(&1);
    let count = u32_at(elst, 4)? as usize;
    let entry = if version1 { 20 } else { 12 };
    for index in 0..count {
        let at = 8 + index * entry;
        let media_time = if version1 {
            u64_at(elst, at + 8)? as i64
        } else {
            i64::from(u32_at(elst, at + 4)? as i32)
        };
        if media_time >= 0 {
            return Ok(media_time as u64);
        }
    }
    Ok(0)
}

/// Every sample's place in the file, size, duration and decode time, from a sample table.
fn sample_table(stbl: &[u8]) -> anyhow::Result<Vec<Sample>> {
    let stsz = child(stbl, b"stsz")?;
    let fixed_size = u32_at(stsz, 4)?;
    let count = u32_at(stsz, 8)? as usize;
    let size = |index: usize| -> anyhow::Result<u32> {
        if fixed_size != 0 {
            Ok(fixed_size)
        } else {
            u32_at(stsz, 12 + index * 4)
        }
    };

    let chunk_offsets: Vec<u64> = if let Ok(stco) = child(stbl, b"stco") {
        (0..u32_at(stco, 4)? as usize)
            .map(|index| Ok(u64::from(u32_at(stco, 8 + index * 4)?)))
            .collect::<anyhow::Result<_>>()?
    } else {
        let co64 = child(stbl, b"co64").context("no chunk offsets")?;
        (0..u32_at(co64, 4)? as usize)
            .map(|index| u64_at(co64, 8 + index * 8))
            .collect::<anyhow::Result<_>>()?
    };

    // (first chunk, counted from 1, samples per chunk)
    let stsc = child(stbl, b"stsc")?;
    let runs: Vec<(usize, usize)> = (0..u32_at(stsc, 4)? as usize)
        .map(|index| {
            Ok((
                u32_at(stsc, 8 + index * 12)? as usize,
                u32_at(stsc, 12 + index * 12)? as usize,
            ))
        })
        .collect::<anyhow::Result<_>>()?;

    let stts = child(stbl, b"stts")?;
    let mut durations = Vec::with_capacity(count);
    for index in 0..u32_at(stts, 4)? as usize {
        let repeat = u32_at(stts, 8 + index * 8)? as usize;
        let duration = u32_at(stts, 12 + index * 8)?;
        durations.extend(std::iter::repeat_n(duration, repeat));
    }
    anyhow::ensure!(durations.len() >= count, "the sample table lacks durations");

    let mut samples = Vec::with_capacity(count);
    let mut time = 0u64;
    for (chunk, &offset) in chunk_offsets.iter().enumerate() {
        let per_chunk = runs
            .iter()
            .rev()
            .find(|(first, _)| *first <= chunk + 1)
            .map_or(0, |(_, per_chunk)| *per_chunk);
        let mut offset = offset;
        for _ in 0..per_chunk {
            let index = samples.len();
            if index == count {
                break;
            }
            let sample = Sample {
                offset,
                size: size(index)?,
                duration: durations[index],
                time,
            };
            offset += u64::from(sample.size);
            time += u64::from(sample.duration);
            samples.push(sample);
        }
    }
    anyhow::ensure!(samples.len() == count, "the sample table does not add up");
    Ok(samples)
}

/// Appends one fragment of the added track: a `moof` describing `samples`, then an `mdat` with their data.
fn fragment(pieces: &mut Vec<Piece>, samples: &[Sample], track_id: u32, sequence: u32) {
    let mfhd = full_box(b"mfhd", 0, 0, &sequence.to_be_bytes());
    // Offsets are counted from the start of the moof.
    let tfhd = full_box(b"tfhd", 0, 0x02_0000, &track_id.to_be_bytes());
    let tfdt = full_box(b"tfdt", 1, 0, &samples[0].time.to_be_bytes());
    let trun_size = 8 + 4 + 4 + 4 + samples.len() * 8;
    let traf_size = 8 + tfhd.len() + tfdt.len() + trun_size;
    let moof_size = 8 + mfhd.len() + traf_size;
    let mut trun = Vec::with_capacity(trun_size - 8);
    trun.extend_from_slice(&(samples.len() as u32).to_be_bytes());
    // The data starts right after the mdat's header.
    trun.extend_from_slice(&(moof_size as u32 + 8).to_be_bytes());
    for sample in samples {
        trun.extend_from_slice(&sample.duration.to_be_bytes());
        trun.extend_from_slice(&sample.size.to_be_bytes());
    }
    let traf = [tfhd, tfdt, full_box(b"trun", 0, 0x00_0301, &trun)].concat();
    let moof = make_box(b"moof", &[mfhd, make_box(b"traf", &traf)].concat());
    debug_assert_eq!(moof.len(), moof_size);
    pieces.push(Piece::Bytes(moof));

    let data: u64 = samples.iter().map(|sample| u64::from(sample.size)).sum();
    let mut mdat = Vec::with_capacity(8);
    mdat.extend_from_slice(&(data as u32 + 8).to_be_bytes());
    mdat.extend_from_slice(b"mdat");
    pieces.push(Piece::Bytes(mdat));
    for sample in samples {
        pieces.push(Piece::Microphone {
            offset: sample.offset,
            length: u64::from(sample.size),
        });
    }
}

/// Joins ranges of the same file that follow each other, and adjacent byte pieces.
fn merge_ranges(pieces: Vec<Piece>) -> Vec<Piece> {
    let mut merged: Vec<Piece> = Vec::with_capacity(pieces.len());
    for piece in pieces {
        match (merged.last_mut(), piece) {
            (
                Some(Piece::Microphone { offset, length }),
                Piece::Microphone {
                    offset: next,
                    length: more,
                },
            ) if *offset + *length == next => {
                *length += more;
            }
            (
                Some(Piece::Recording { offset, length }),
                Piece::Recording {
                    offset: next,
                    length: more,
                },
            ) if *offset + *length == next => {
                *length += more;
            }
            (Some(Piece::Bytes(bytes)), Piece::Bytes(more)) => bytes.extend_from_slice(&more),
            (_, piece) => merged.push(piece),
        }
    }
    merged
}

/// Writes `pieces`, copying the ranges from the two files.
pub fn write_pieces(
    pieces: &[Piece],
    recording: &dyn ReadAt,
    microphone: &dyn ReadAt,
    out: &mut dyn io::Write,
) -> io::Result<()> {
    let mut buffer = vec![0u8; 1 << 20];
    for piece in pieces {
        let (file, mut offset, mut remaining) = match piece {
            Piece::Bytes(bytes) => {
                out.write_all(bytes)?;
                continue;
            }
            Piece::Recording { offset, length } => (recording, *offset, *length),
            Piece::Microphone { offset, length } => (microphone, *offset, *length),
        };
        while remaining > 0 {
            let step = remaining.min(buffer.len() as u64) as usize;
            file.read_at(offset, &mut buffer[..step])?;
            out.write_all(&buffer[..step])?;
            offset += step as u64;
            remaining -= step as u64;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32s(values: &[u32]) -> Vec<u8> {
        values.iter().flat_map(|value| value.to_be_bytes()).collect()
    }

    /// A minimal fragmented recording: one audio track (ID 1, timescale 48000) in two fragments of two 1024-sample
    /// frames, starting at 0 and at 2048, each frame's data four bytes of its fragment's number.
    fn recording() -> Vec<u8> {
        recording_saying_when_fragments_start(true)
    }

    /// The same, with or without a `tfdt` in each fragment.
    fn recording_saying_when_fragments_start(with_tfdt: bool) -> Vec<u8> {
        let tkhd = full_box(b"tkhd", 0, 3, &[u32s(&[0, 0, 1, 0, 0]), vec![0; 60]].concat());
        let mdhd = full_box(b"mdhd", 0, 0, &u32s(&[0, 0, 48_000, 0, 0]));
        let hdlr = full_box(b"hdlr", 0, 0, &[u32s(&[0]), b"soun".to_vec(), vec![0; 13]].concat());
        let stbl = make_box(b"stbl", &[full_box(b"stco", 0, 0, &u32s(&[0]))].concat());
        let minf = make_box(b"minf", &stbl);
        let mdia = make_box(b"mdia", &[mdhd, hdlr, minf].concat());
        let trak = make_box(b"trak", &[tkhd, mdia].concat());
        let mvhd = full_box(b"mvhd", 0, 0, &[vec![0; 92], u32s(&[0])].concat());
        let mvex = make_box(b"mvex", &full_box(b"trex", 0, 0, &u32s(&[1, 1, 0, 0, 0])));
        let moov = make_box(b"moov", &[mvhd, trak, mvex].concat());
        let mut file = [make_box(b"ftyp", b"iso5\0\0\0\0"), moov].concat();
        for (sequence, start) in [(1u32, 0u32), (2, 2048)] {
            let mfhd = full_box(b"mfhd", 0, 0, &u32s(&[sequence]));
            let tfhd = full_box(b"tfhd", 0, 0x02_0000, &u32s(&[1]));
            let tfdt = if with_tfdt {
                full_box(b"tfdt", 0, 0, &u32s(&[start]))
            } else {
                Vec::new()
            };
            let trun = full_box(b"trun", 0, 0x301, &u32s(&[2, 0, 1024, 4, 1024, 4]));
            let moof = make_box(
                b"moof",
                &[mfhd, make_box(b"traf", &[tfhd, tfdt, trun].concat())].concat(),
            );
            file.extend(moof);
            file.extend(make_box(b"mdat", &[sequence as u8; 8]));
        }
        file
    }

    /// An ordinary microphone recording: ten 1024-sample frames of two bytes (the frame's index twice), all in one
    /// mdat at the start, with 2112 samples of priming in its edit list.
    fn microphone() -> Vec<u8> {
        let data: Vec<u8> = (0..10u8).flat_map(|index| [index, index]).collect();
        let mdat = make_box(b"mdat", &data);
        let tkhd = full_box(b"tkhd", 0, 3, &[u32s(&[0, 0, 1, 0, 480]), vec![0; 60]].concat());
        let elst = full_box(b"elst", 0, 0, &u32s(&[1, 240, 2112, 0x0001_0000]));
        let mdhd = full_box(b"mdhd", 0, 0, &u32s(&[0, 0, 48_000, 10_240, 0]));
        let hdlr = full_box(b"hdlr", 0, 0, &[u32s(&[0]), b"soun".to_vec(), vec![0; 13]].concat());
        let stsd = full_box(b"stsd", 0, 0, &[u32s(&[1]), make_box(b"mp4a", &[0; 28])].concat());
        let stts = full_box(b"stts", 0, 0, &u32s(&[1, 10, 1024]));
        let stsc = full_box(b"stsc", 0, 0, &u32s(&[2, 1, 6, 1, 2, 4, 1]));
        let stsz = full_box(b"stsz", 0, 0, &u32s(&[2, 10]));
        let stco = full_box(b"stco", 0, 0, &u32s(&[2, 8, 20]));
        let stbl = make_box(b"stbl", &[stsd, stts, stsc, stsz, stco].concat());
        let minf = make_box(
            b"minf",
            &[full_box(b"smhd", 0, 0, &[0; 4]), make_box(b"dinf", &[]), stbl].concat(),
        );
        let mdia = make_box(b"mdia", &[mdhd, hdlr, minf].concat());
        let trak = make_box(b"trak", &[tkhd, make_box(b"edts", &elst), mdia].concat());
        [mdat, make_box(b"moov", &trak)].concat()
    }

    /// A microphone recording in fragments: ten 1024-sample frames of two bytes (the frame's index twice), three to a
    /// fragment, in track 2 and without an edit list.
    ///
    /// With `start`, as a browser writes fragments: each says when it starts (`tfdt`, the first at `start`) and
    /// addresses its data from itself. Without, as Media Foundation writes them: no `tfdt`, and the data addressed by
    /// an explicit offset from the start of the file, with a fragment index at the end.
    fn fragmented_microphone(start: Option<u32>) -> Vec<u8> {
        let tkhd = full_box(b"tkhd", 0, 1, &[u32s(&[0, 0, 2, 0, 0]), vec![0; 60]].concat());
        let mdhd = full_box(b"mdhd", 0, 0, &u32s(&[0, 0, 48_000, 0, 0]));
        let hdlr = full_box(b"hdlr", 0, 0, &[u32s(&[0]), b"soun".to_vec(), vec![0; 13]].concat());
        let stsd = full_box(b"stsd", 0, 0, &[u32s(&[1]), make_box(b"mp4a", &[0; 28])].concat());
        let empty = |kind: &[u8; 4], fields: usize| full_box(kind, 0, 0, &vec![0u8; fields]);
        let stbl = make_box(
            b"stbl",
            &[
                stsd,
                empty(b"stts", 4),
                empty(b"stsc", 4),
                empty(b"stsz", 8),
                empty(b"stco", 4),
            ]
            .concat(),
        );
        let minf = make_box(
            b"minf",
            &[full_box(b"smhd", 0, 0, &[0; 4]), make_box(b"dinf", &[]), stbl].concat(),
        );
        let mdia = make_box(b"mdia", &[mdhd, hdlr, minf].concat());
        let trak = make_box(b"trak", &[tkhd, mdia].concat());
        let mvhd = full_box(b"mvhd", 0, 0, &[vec![0; 92], u32s(&[3])].concat());
        let mvex = make_box(b"mvex", &full_box(b"trex", 0, 0, &u32s(&[2, 1, 0, 0, 0x1_0000])));
        let mut file = [
            make_box(b"ftyp", b"iso5\0\0\0\0"),
            make_box(b"moov", &[mvhd, trak, mvex].concat()),
        ]
        .concat();

        let frames: Vec<u8> = (0..10u8).collect();
        for (index, chunk) in frames.chunks(3).enumerate() {
            let mfhd = full_box(b"mfhd", 0, 0, &u32s(&[index as u32 + 1]));
            let sizes: Vec<u32> = chunk.iter().flat_map(|_| [1024, 2]).collect();
            let moof = |data_at: u64| {
                let (tfhd, tfdt, data_offset) = match start {
                    Some(start) => (
                        full_box(b"tfhd", 0, 0x02_0000, &u32s(&[2])),
                        full_box(b"tfdt", 0, 0, &u32s(&[start + index as u32 * 3 * 1024])),
                        data_at as u32,
                    ),
                    None => (
                        full_box(b"tfhd", 0, 0x01, &[u32s(&[2]), data_at.to_be_bytes().to_vec()].concat()),
                        Vec::new(),
                        0,
                    ),
                };
                let trun = full_box(
                    b"trun",
                    0,
                    0x301,
                    &[u32s(&[chunk.len() as u32, data_offset]), u32s(&sizes)].concat(),
                );
                make_box(
                    b"moof",
                    &[mfhd.clone(), make_box(b"traf", &[tfhd, tfdt, trun].concat())].concat(),
                )
            };
            // The data follows the moof and the mdat's header: from the moof, or from the start of the file.
            let size = moof(0).len() as u64;
            let data_at = if start.is_some() {
                size + 8
            } else {
                file.len() as u64 + size + 8
            };
            file.extend(moof(data_at));
            let data: Vec<u8> = chunk.iter().flat_map(|&frame| [frame, frame]).collect();
            file.extend(make_box(b"mdat", &data));
        }
        if start.is_none() {
            file.extend(make_box(b"mfra", &[]));
        }
        file
    }

    fn written(placement: Placement) -> anyhow::Result<Vec<u8>> {
        merged(&recording(), &microphone(), placement)
    }

    fn merged(recording: &Vec<u8>, microphone: &Vec<u8>, placement: Placement) -> anyhow::Result<Vec<u8>> {
        let pieces = add_audio_track(recording, microphone, placement)?;
        let mut out = Vec::new();
        write_pieces(&pieces, recording, microphone, &mut out)?;
        Ok(out)
    }

    const NOWHERE: Placement = Placement {
        skip: Duration::ZERO,
        delay: Duration::ZERO,
    };

    /// (sequence, track, decode time, sample sizes, data) of a fragment.
    type Summary = (u32, u32, u64, Vec<u32>, Vec<u8>);

    /// Every fragment in `file`.
    fn fragments(file: &[u8]) -> Vec<Summary> {
        let boxes = children(file).unwrap();
        let mut found = Vec::new();
        for (index, (kind, whole, moof)) in boxes.iter().enumerate() {
            if kind != b"moof" {
                continue;
            }
            let traf = child(moof, b"traf").unwrap();
            let time = match child(traf, b"tfdt") {
                Ok(tfdt) if tfdt[0] == 1 => u64_at(tfdt, 4).unwrap(),
                Ok(tfdt) => u64::from(u32_at(tfdt, 4).unwrap()),
                // A fragment that does not say.
                Err(_) => u64::MAX,
            };
            let trun = child(traf, b"trun").unwrap();
            let count = u32_at(trun, 4).unwrap() as usize;
            let data_offset = u32_at(trun, 8).unwrap() as usize;
            let sizes: Vec<u32> = (0..count).map(|i| u32_at(trun, 12 + i * 8 + 4).unwrap()).collect();
            // The data, found through the offset relative to the moof, which must land in the following mdat.
            let moof_start = whole.as_ptr() as usize - file.as_ptr() as usize;
            let total: usize = sizes.iter().map(|size| *size as usize).sum();
            let data = file[moof_start + data_offset..moof_start + data_offset + total].to_vec();
            assert_eq!(boxes[index + 1].0, *b"mdat");
            found.push((
                u32_at(child(moof, b"mfhd").unwrap(), 4).unwrap(),
                u32_at(child(traf, b"tfhd").unwrap(), 4).unwrap(),
                time,
                sizes,
                data,
            ));
        }
        found
    }

    fn tracks(file: &[u8]) -> Vec<u32> {
        let moov = child(file, b"moov").unwrap();
        children(moov)
            .unwrap()
            .into_iter()
            .filter(|(kind, _, _)| kind == b"trak")
            .map(|(_, _, trak)| track_id(child(trak, b"tkhd").unwrap()).unwrap())
            .collect()
    }

    #[test]
    fn the_recording_keeps_its_fragments_byte_for_byte_and_gains_a_track() {
        let out = written(NOWHERE).unwrap();
        let original = recording();

        assert_eq!(tracks(&out), [1, 2]);
        let moov = child(&out, b"moov").unwrap();
        let mvhd = child(moov, b"mvhd").unwrap();
        assert_eq!(u32_at(mvhd, mvhd.len() - 4).unwrap(), 3, "the next free track ID");
        let trex: Vec<u32> = children(child(moov, b"mvex").unwrap())
            .unwrap()
            .into_iter()
            .map(|(_, _, trex)| u32_at(trex, 4).unwrap())
            .collect();
        assert_eq!(trex, [1, 2]);

        // Everything after the moov in the original follows the new moov unchanged.
        let original_tail = &original[original.windows(4).position(|w| w == b"moof").unwrap() - 4..];
        let new_moov_end = out.windows(4).position(|w| w == b"moof").unwrap() - 4;
        assert_eq!(&out[new_moov_end..new_moov_end + original_tail.len()], original_tail);
    }

    #[test]
    fn the_microphone_follows_in_fragments_of_its_own_without_its_priming() {
        let out = written(NOWHERE).unwrap();
        let added: Vec<_> = fragments(&out).into_iter().filter(|f| f.1 == 2).collect();

        // The recording lasts 4096 samples; priming of 2112 drops frames that start before it (0, 1024, 2048), so
        // frames 3.. start at 960, 1984, 3008 on the recording's timeline, and 4032; 5056 is past the end.
        assert_eq!(added.len(), 1);
        let (sequence, _, time, sizes, data) = &added[0];
        assert_eq!(*sequence, 3, "after the recording's fragments");
        assert_eq!(*time, 3 * 1024 - 2112);
        assert_eq!(sizes, &[2, 2, 2, 2]);
        assert_eq!(data, &[3, 3, 4, 4, 5, 5, 6, 6]);
    }

    #[test]
    fn a_skipped_start_is_dropped_and_a_delay_moves_the_track() {
        let skipped = Placement {
            skip: Duration::from_nanos(1024 * 1_000_000_000 / 48_000),
            delay: Duration::ZERO,
        };
        let added: Vec<_> = fragments(&written(skipped).unwrap())
            .into_iter()
            .filter(|f| f.1 == 2)
            .collect();
        assert_eq!(added[0].2, 4 * 1024 - 2112 - 1024);
        assert_eq!(added[0].4[..2], [4, 4]);

        let delayed = Placement {
            skip: Duration::ZERO,
            delay: Duration::from_nanos(1000 * 1_000_000_000 / 48_000),
        };
        let added: Vec<_> = fragments(&written(delayed).unwrap())
            .into_iter()
            .filter(|f| f.1 == 2)
            .collect();
        assert_eq!(added[0].2, 3 * 1024 - 2112 + 1000);
        assert_eq!(
            added[0].3.len(),
            3,
            "the frame that would start at 5032 is past the end"
        );
    }

    /// The fragments of the track added to the recording (track 2).
    fn added(out: &[u8]) -> Vec<Summary> {
        fragments(out).into_iter().filter(|f| f.1 == 2).collect()
    }

    #[test]
    fn a_microphone_in_fragments_as_media_foundation_writes_them_is_added_like_an_ordinary_one() {
        let (recording, microphone) = (recording(), fragmented_microphone(None));

        // No priming: frames 0 to 3 start at 0, 1024, 2048 and 3072 of the 4096 samples the recording lasts. Frame 3
        // is the first of the microphone's second fragment.
        let all = added(&merged(&recording, &microphone, NOWHERE).unwrap());
        assert_eq!(all.len(), 1);
        let (_, _, time, sizes, data) = &all[0];
        assert_eq!(*time, 0);
        assert_eq!(sizes, &[2, 2, 2, 2]);
        assert_eq!(data, &[0, 0, 1, 1, 2, 2, 3, 3]);

        let skipped = Placement {
            skip: Duration::from_nanos(1024 * 1_000_000_000 / 48_000),
            delay: Duration::ZERO,
        };
        let all = added(&merged(&recording, &microphone, skipped).unwrap());
        assert_eq!(all[0].2, 0);
        assert_eq!(all[0].4, [1, 1, 2, 2, 3, 3, 4, 4]);
    }

    #[test]
    fn a_microphone_in_fragments_that_say_when_they_start_keeps_those_times() {
        let all = added(&merged(&recording(), &fragmented_microphone(Some(1024)), NOWHERE).unwrap());

        // Frames start at 1024, 2048 and 3072; the fourth, at 4096, is past the end of the recording.
        assert_eq!(all[0].2, 1024);
        assert_eq!(all[0].4, [0, 0, 1, 1, 2, 2]);
    }

    #[test]
    fn a_recording_whose_fragments_do_not_say_when_they_start_lasts_as_long_as_its_samples() {
        let recording = recording_saying_when_fragments_start(false);

        // Its second fragment follows the first, so it lasts 4096 samples, as with a tfdt in each: the microphone's
        // frames 3 to 6 start before then (see the test with priming).
        let all = added(&merged(&recording, &microphone(), NOWHERE).unwrap());
        assert_eq!(all[0].2, 3 * 1024 - 2112);
        assert_eq!(all[0].3.len(), 4);
    }

    #[test]
    fn a_recording_addressed_from_the_start_of_the_file_finds_its_data_where_its_offsets_say() {
        // As Media Foundation writes one: its fragments' data at explicit offsets, and a fragment index at the end.
        let out = merged(&fragmented_microphone(None), &microphone(), NOWHERE).unwrap();

        assert_eq!(tracks(&out), [2, 3]);
        let boxes = children(&out).unwrap();
        assert_eq!(boxes.last().unwrap().0, *b"mfra", "the fragment index stays last");
        let mut data = Vec::new();
        for (kind, _, moof) in &boxes {
            let Some(traf) = (kind == b"moof").then(|| child(moof, b"traf").unwrap()) else {
                continue;
            };
            let tfhd = child(traf, b"tfhd").unwrap();
            if u32_at(tfhd, 4).unwrap() == 2 {
                let base = u64_at(tfhd, 8).unwrap() as usize;
                let count = u32_at(child(traf, b"trun").unwrap(), 4).unwrap() as usize;
                data.extend_from_slice(&out[base..base + count * 2]);
            }
        }
        let frames: Vec<u8> = (0..10u8).flat_map(|frame| [frame, frame]).collect();
        assert_eq!(data, frames);
        assert_eq!(
            fragments(&out).into_iter().filter(|f| f.1 == 3).count(),
            1,
            "the microphone follows"
        );
    }

    #[test]
    fn a_microphone_that_does_not_overlap_is_refused() {
        let late = Placement {
            skip: Duration::ZERO,
            delay: Duration::from_secs(1),
        };
        assert!(written(late).unwrap_err().to_string().contains("does not overlap"));
    }

    #[test]
    fn a_recording_that_is_not_fragmented_is_refused() {
        let microphone = microphone();
        let error = add_audio_track(&microphone, &microphone, NOWHERE).unwrap_err();
        assert!(error.to_string().contains("not a fragmented MP4"), "{error:#}");
    }

    #[test]
    fn a_truncated_file_is_refused() {
        let recording = recording();
        let cut = recording[..recording.len() - 3].to_vec();
        assert!(add_audio_track(&cut, &microphone(), NOWHERE).is_err());
    }

    fn entries(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn with_written_metadata(file: &Vec<u8>, pairs: &[(&str, &str)]) -> Vec<u8> {
        let pieces = with_metadata(file, &entries(pairs)).unwrap();
        let mut out = Vec::new();
        write_pieces(&pieces, file, &Vec::new(), &mut out).unwrap();
        out
    }

    /// The (key, value) pairs of the QuickTime metadata right in the `moov`, and how many such `meta` boxes there are.
    fn metadata_of(file: &[u8]) -> (Vec<(String, String)>, usize) {
        let moov = child(file, b"moov").unwrap();
        let metas: Vec<_> = children(moov)
            .unwrap()
            .into_iter()
            .filter(|(kind, _, _)| kind == b"meta")
            .collect();
        let Some((_, _, meta)) = metas.first() else {
            return (Vec::new(), 0);
        };
        assert_eq!(&child(meta, b"hdlr").unwrap()[8..12], b"mdta");
        let keys = child(meta, b"keys").unwrap();
        let names: Vec<String> = children(&keys[8..])
            .unwrap()
            .into_iter()
            .map(|(kind, _, name)| {
                assert_eq!(&kind, b"mdta");
                String::from_utf8(name.to_vec()).unwrap()
            })
            .collect();
        assert_eq!(u32_at(keys, 4).unwrap() as usize, names.len());
        let values: Vec<String> = children(child(meta, b"ilst").unwrap())
            .unwrap()
            .into_iter()
            .enumerate()
            .map(|(index, (kind, _, item))| {
                assert_eq!(u32::from_be_bytes(kind), index as u32 + 1);
                let data = child(item, b"data").unwrap();
                assert_eq!(u32_at(data, 0).unwrap(), 1, "UTF-8");
                String::from_utf8(data[8..].to_vec()).unwrap()
            })
            .collect();
        (names.into_iter().zip(values).collect(), metas.len())
    }

    #[test]
    fn metadata_goes_into_the_movie_header_and_every_fragment_is_copied_as_it_was() {
        let original = recording();
        let written = with_written_metadata(&original, &[("com.meeting-assistant.app", "GoogleMeet")]);

        assert_eq!(
            metadata_of(&written),
            (entries(&[("com.meeting-assistant.app", "GoogleMeet")]), 1)
        );
        let moov_end = |file: &[u8]| {
            let boxes = children(file).unwrap();
            let index = boxes.iter().position(|(kind, _, _)| kind == b"moov").unwrap();
            file.len()
                - boxes[index + 1..]
                    .iter()
                    .map(|(_, whole, _)| whole.len())
                    .sum::<usize>()
        };
        assert_eq!(
            written[moov_end(&written)..],
            original[moov_end(&original)..],
            "fragments address their data from themselves"
        );
        assert_eq!(fragments(&written), fragments(&original));
    }

    /// An ordinary file of one track with its four bytes of data in one chunk: `moov` before or after `mdat`.
    fn ordinary(moov_first: bool) -> Vec<u8> {
        let ftyp = make_box(b"ftyp", b"mp42\0\0\0\0");
        let data = *b"DATA";
        let build = |offset: u32| {
            let stbl = make_box(b"stbl", &full_box(b"stco", 0, 0, &u32s(&[1, offset])));
            let mdia = make_box(b"mdia", &make_box(b"minf", &stbl));
            make_box(b"moov", &make_box(b"trak", &mdia))
        };
        if moov_first {
            let size = build(0).len();
            let offset = (ftyp.len() + size + 8) as u32;
            [ftyp, build(offset), make_box(b"mdat", &data)].concat()
        } else {
            let offset = (ftyp.len() + 8) as u32;
            [ftyp, make_box(b"mdat", &data), build(offset)].concat()
        }
    }

    fn chunk_offset(file: &[u8]) -> usize {
        let moov = child(file, b"moov").unwrap();
        let stbl = child(
            child(child(child(moov, b"trak").unwrap(), b"mdia").unwrap(), b"minf").unwrap(),
            b"stbl",
        )
        .unwrap();
        u32_at(child(stbl, b"stco").unwrap(), 8).unwrap() as usize
    }

    #[test]
    fn chunk_offsets_past_the_movie_header_move_with_it_and_others_stay() {
        for moov_first in [true, false] {
            let original = ordinary(moov_first);
            let written = with_written_metadata(&original, &[("com.meeting-assistant.app", "Slack")]);
            let offset = chunk_offset(&written);
            assert_eq!(&written[offset..offset + 4], b"DATA", "moov first: {moov_first}");
            if !moov_first {
                assert_eq!(offset, chunk_offset(&original), "a moov written last moves nothing");
            }
        }
    }

    #[test]
    fn explicit_fragment_offsets_and_the_fragment_index_move_with_the_movie_header() {
        let ftyp = make_box(b"ftyp", b"iso5\0\0\0\0");
        let moov = make_box(b"moov", &full_box(b"mvhd", 0, 0, &[0; 96]));
        let moof_at = (ftyp.len() + moov.len()) as u64;
        let moof_for = |base: u64| {
            let tfhd = full_box(b"tfhd", 0, 1, &[u32s(&[1]), base.to_be_bytes().to_vec()].concat());
            make_box(b"moof", &make_box(b"traf", &tfhd))
        };
        let data_at = moof_at + moof_for(0).len() as u64 + 8;
        let tfra = full_box(
            b"tfra",
            1,
            0,
            &[
                u32s(&[1, 0, 1]),
                0u64.to_be_bytes().to_vec(),
                moof_at.to_be_bytes().to_vec(),
                vec![1, 1, 1],
            ]
            .concat(),
        );
        let original = [
            ftyp,
            moov,
            moof_for(data_at),
            make_box(b"mdat", b"DATA"),
            make_box(b"mfra", &tfra),
        ]
        .concat();

        let written = with_written_metadata(&original, &[("com.meeting-assistant.app", "Teams")]);

        let moof = child(&written, b"moof").unwrap();
        let base = u64_at(child(child(moof, b"traf").unwrap(), b"tfhd").unwrap(), 8).unwrap() as usize;
        assert_eq!(&written[base..base + 4], b"DATA");
        let indexed = u64_at(child(child(&written, b"mfra").unwrap(), b"tfra").unwrap(), 24).unwrap() as usize;
        assert_eq!(&written[indexed + 4..indexed + 8], b"moof");
    }

    #[test]
    fn metadata_written_again_replaces_what_was_written() {
        let once = with_written_metadata(&recording(), &[("com.meeting-assistant.app", "Zoom")]);
        let twice = with_written_metadata(
            &once,
            &[
                ("com.meeting-assistant.app", "Zoom"),
                ("com.meeting-assistant.started_at", "2026-11-19T19:01:56+09:00"),
            ],
        );
        let (pairs, boxes) = metadata_of(&twice);
        assert_eq!(boxes, 1);
        assert_eq!(pairs.len(), 2);
        assert_eq!(fragments(&twice), fragments(&recording()));
    }
}
