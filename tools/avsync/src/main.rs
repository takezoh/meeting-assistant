//! A/V sync measurement for screen recordings.
//!
//! `avsync stimulus <seconds> [<period>]`: flashes a small topmost square at the top-left corner of the primary display
//! white and plays a 1 kHz tone burst on the default output device at the same moment, once per period.
//!
//! `avsync analyze <file.mp4> [<x> <y> <size>]`: decodes the recording, finds when the square turns white and when the
//! tone starts, pairs them and prints audio-minus-video offsets over time.
//!
//! `avsync tracks <file.mp4>`: lists the tracks in the order the file stores them and the metadata keys, then decodes
//! every stream and prints, for each audio track and channel, its peak, its mean level and when it first gets loud,
//! and for the video where in the picture something other than black is, each time that changes.

use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Media::Audio::*;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::Media::timeBeginPeriod;
use windows::Win32::System::Com::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

const SQUARE: i32 = 160;
const RATE: u32 = 48_000;
const TONE_HZ: f64 = 1000.0;
const TONE: Duration = Duration::from_millis(60);
const FLASH: Duration = Duration::from_millis(200);
const WM_FLASH: u32 = WM_APP + 1;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("stimulus") => {
            let seconds: u64 = args.get(1).context("seconds")?.parse()?;
            let period: f64 = args.get(2).map_or(Ok(2.0), |p| p.parse())?;
            stimulus(Duration::from_secs(seconds), Duration::from_secs_f64(period))
        }
        Some("analyze") => {
            let path = args.get(1).context("file")?;
            let region = match args.get(2..5) {
                Some([x, y, size]) => (x.parse()?, y.parse()?, size.parse()?),
                _ => (24u32, 24u32, 112u32),
            };
            analyze(path, region)
        }
        Some("tracks") => tracks(args.get(1).context("file")?),
        _ => bail!(
            "usage: avsync stimulus <seconds> [<period>] | avsync analyze <file> [<x> <y> <size>] | avsync tracks <file>"
        ),
    }
}

// ---------------------------------------------------------------- stimulus

static mut WHITE: bool = false;

extern "system" fn window_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match message {
            WM_FLASH => {
                WHITE = wparam.0 != 0;
                let _ = InvalidateRect(Some(window), None, false);
                let _ = UpdateWindow(window);
                LRESULT(0)
            }
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let dc = BeginPaint(window, &mut paint);
                let brush = GetStockObject(if WHITE { WHITE_BRUSH } else { BLACK_BRUSH });
                let mut rect = RECT::default();
                let _ = GetClientRect(window, &mut rect);
                FillRect(dc, &rect, HBRUSH(brush.0));
                let _ = EndPaint(window, &paint);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(window, message, wparam, lparam),
        }
    }
}

fn stimulus(length: Duration, period: Duration) -> anyhow::Result<()> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        timeBeginPeriod(1);
        let instance = GetModuleHandleW(None)?;
        let class = w!("avsync-square");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        });
        let window = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
            w!("avsync"),
            WS_POPUP | WS_VISIBLE,
            0,
            0,
            SQUARE,
            SQUARE,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        let window_value = window.0 as isize;

        let audio = std::thread::spawn(move || -> anyhow::Result<()> {
            let window = HWND(window_value as *mut _);
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
            let format = WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_PCM as u16,
                nChannels: 2,
                nSamplesPerSec: RATE,
                nAvgBytesPerSec: RATE * 4,
                nBlockAlign: 4,
                wBitsPerSample: 16,
                cbSize: 0,
            };
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                500_000,
                0,
                &format,
                None,
            )?;
            let render: IAudioRenderClient = client.GetService()?;
            let buffer_frames = client.GetBufferSize()?;
            client.Start()?;

            // Keep about 15 ms queued ahead of the device.
            let ahead = RATE * 15 / 1000;
            let tone_frames = (TONE.as_secs_f64() * RATE as f64) as u64;
            let started = Instant::now();
            let mut next_event = started + Duration::from_secs(1);
            let mut flash_off: Option<Instant> = None;
            let mut tone_position: Option<u64> = None;
            let mut events = 0;
            while started.elapsed() < length {
                let now = Instant::now();
                if now >= next_event {
                    tone_position = Some(0);
                    PostMessageW(Some(window), WM_FLASH, WPARAM(1), LPARAM(0))?;
                    flash_off = Some(now + FLASH);
                    next_event += period;
                    events += 1;
                }
                if flash_off.is_some_and(|off| now >= off) {
                    PostMessageW(Some(window), WM_FLASH, WPARAM(0), LPARAM(0))?;
                    flash_off = None;
                }
                let padding = client.GetCurrentPadding()?;
                let wanted = ahead.saturating_sub(padding).min(buffer_frames - padding);
                if wanted > 0 {
                    let data = render.GetBuffer(wanted)?.cast::<i16>();
                    let samples = std::slice::from_raw_parts_mut(data, wanted as usize * 2);
                    for frame in samples.chunks_mut(2) {
                        let value = match tone_position {
                            Some(position) if position < tone_frames => {
                                tone_position = Some(position + 1);
                                let t = position as f64 / RATE as f64;
                                // 2 ms ramps keep the burst from clicking.
                                let ramp = (t / 0.002).min((TONE.as_secs_f64() - t) / 0.002).min(1.0);
                                ((2.0 * std::f64::consts::PI * TONE_HZ * t).sin() * 0.5 * ramp * i16::MAX as f64) as i16
                            }
                            _ => 0,
                        };
                        frame[0] = value;
                        frame[1] = value;
                    }
                    render.ReleaseBuffer(wanted, 0)?;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            client.Stop()?;
            println!("stimulus: {events} events every {:.2} s", period.as_secs_f64());
            let _ = PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0));
            Ok(())
        });

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        audio.join().unwrap()
    }
}

// ---------------------------------------------------------------- analysis

fn analyze(path: &str, (x, y, size): (u32, u32, u32)) -> anyhow::Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        MFStartup(MF_VERSION, MFSTARTUP_FULL)?;

        let video = video_onsets(path, (x, y, size))?;
        let audio = audio_onsets(path)?;
        println!("video flashes: {}, audio tones: {}", video.len(), audio.len());

        let mut pairs = Vec::new();
        for &v in &video {
            if let Some(&a) = audio
                .iter()
                .min_by(|a, b| (*a - v).abs().partial_cmp(&(*b - v).abs()).unwrap())
                && (a - v).abs() < 0.9
            {
                pairs.push((v, (a - v) * 1000.0));
            }
        }
        if pairs.is_empty() {
            bail!("no flash and tone pairs found");
        }
        println!("pairs: {}", pairs.len());
        println!("time_s,offset_ms (positive: audio later than video)");
        for (time, offset) in &pairs {
            println!("{time:.3},{offset:.1}");
        }
        let n = pairs.len() as f64;
        let mean = pairs.iter().map(|p| p.1).sum::<f64>() / n;
        let first: Vec<f64> = pairs.iter().take(10).map(|p| p.1).collect();
        let last: Vec<f64> = pairs.iter().rev().take(10).map(|p| p.1).collect();
        let avg = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
        let mean_t = pairs.iter().map(|p| p.0).sum::<f64>() / n;
        let slope = pairs.iter().map(|p| (p.0 - mean_t) * (p.1 - mean)).sum::<f64>()
            / pairs.iter().map(|p| (p.0 - mean_t).powi(2)).sum::<f64>().max(1e-9);
        let min = pairs.iter().map(|p| p.1).fold(f64::MAX, f64::min);
        let max = pairs.iter().map(|p| p.1).fold(f64::MIN, f64::max);
        println!(
            "summary: mean {mean:.1} ms, first10 {:.1} ms, last10 {:.1} ms, min {min:.1}, max {max:.1}, \
             trend {:.2} ms/min",
            avg(&first),
            avg(&last),
            slope * 60.0
        );
        // The video side is quantized to frames, so medians per minute show the course best.
        let mut minutes: Vec<Vec<f64>> = Vec::new();
        for &(time, offset) in &pairs {
            let minute = (time / 60.0) as usize;
            if minutes.len() <= minute {
                minutes.resize(minute + 1, Vec::new());
            }
            minutes[minute].push(offset);
        }
        let per_minute: Vec<String> = minutes
            .iter_mut()
            .map(|values| {
                values.sort_by(|a, b| a.partial_cmp(b).unwrap());
                values
                    .get(values.len() / 2)
                    .map_or("-".to_owned(), |median| format!("{median:.0}"))
            })
            .collect();
        println!("median per minute (ms): {}", per_minute.join(" "));
        Ok(())
    }
}

unsafe fn reader(path: &str) -> anyhow::Result<IMFSourceReader> {
    unsafe {
        let mut attributes = None;
        MFCreateAttributes(&mut attributes, 1)?;
        let attributes = attributes.unwrap();
        attributes.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)?;
        Ok(MFCreateSourceReaderFromURL(&HSTRING::from(path), &attributes)?)
    }
}

const FIRST_VIDEO: u32 = 0xFFFF_FFFC;
const FIRST_AUDIO: u32 = 0xFFFF_FFFD;

/// Times (s) at which the square's mean luma goes from dark to bright.
unsafe fn video_onsets(path: &str, (x, y, size): (u32, u32, u32)) -> anyhow::Result<Vec<f64>> {
    unsafe {
        let reader = reader(path)?;
        reader.SetStreamSelection(0xFFFF_FFFE, false)?;
        reader.SetStreamSelection(FIRST_VIDEO, true)?;
        let media_type = MFCreateMediaType()?;
        media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        media_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
        reader.SetCurrentMediaType(FIRST_VIDEO, None, &media_type)?;

        let mut onsets = Vec::new();
        let mut bright = true; // Require a dark frame first.
        let mut frames = 0u64;
        loop {
            let mut flags = 0u32;
            let mut time = 0i64;
            let mut sample = None;
            reader.ReadSample(
                FIRST_VIDEO,
                0,
                None,
                Some(&mut flags),
                Some(&mut time),
                Some(&mut sample),
            )?;
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                break;
            }
            let Some(sample) = sample else { continue };
            frames += 1;
            let buffer = sample.ConvertToContiguousBuffer()?;
            let buffer2d: IMF2DBuffer = buffer.cast()?;
            let mut scan0 = std::ptr::null_mut();
            let mut pitch = 0i32;
            buffer2d.Lock2D(&mut scan0, &mut pitch)?;
            let mut sum = 0u64;
            for row in y..y + size {
                let line = scan0.offset(row as isize * pitch as isize);
                for col in x..x + size {
                    sum += *line.add(col as usize) as u64;
                }
            }
            buffer2d.Unlock2D()?;
            let luma = sum / (size as u64 * size as u64);
            if !bright && luma > 160 {
                bright = true;
                onsets.push(time as f64 / 1e7);
            } else if bright && luma < 60 {
                bright = false;
            }
        }
        eprintln!("video frames decoded: {frames}");
        Ok(onsets)
    }
}

/// Times (s) at which a 1 kHz tone starts after at least 0.5 s without it.
unsafe fn audio_onsets(path: &str) -> anyhow::Result<Vec<f64>> {
    unsafe {
        let reader = reader(path)?;
        reader.SetStreamSelection(0xFFFF_FFFE, false)?;
        reader.SetStreamSelection(FIRST_AUDIO, true)?;
        let media_type = MFCreateMediaType()?;
        media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        media_type.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
        media_type.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        reader.SetCurrentMediaType(FIRST_AUDIO, None, &media_type)?;
        let current = reader.GetCurrentMediaType(FIRST_AUDIO)?;
        let rate = current.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)? as f64;
        let channels = current.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)? as usize;

        // Collect the left channel with the time of its first sample.
        let mut start: Option<f64> = None;
        let mut mono: Vec<f32> = Vec::new();
        loop {
            let mut flags = 0u32;
            let mut time = 0i64;
            let mut sample = None;
            reader.ReadSample(
                FIRST_AUDIO,
                0,
                None,
                Some(&mut flags),
                Some(&mut time),
                Some(&mut sample),
            )?;
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                break;
            }
            let Some(sample) = sample else { continue };
            let t = time as f64 / 1e7;
            let expected = start.map(|s| s + mono.len() as f64 / rate);
            match (start, expected) {
                (None, _) => start = Some(t),
                (Some(_), Some(e)) if (t - e).abs() > 0.002 => {
                    eprintln!("audio timestamp jump at {e:.3}s -> {t:.3}s");
                    // Pad or trim to keep sample positions on the timestamps.
                    let target = ((t - start.unwrap()) * rate).round() as usize;
                    mono.resize(target, 0.0);
                }
                _ => {}
            }
            let buffer = sample.ConvertToContiguousBuffer()?;
            let mut data = std::ptr::null_mut();
            let mut length = 0u32;
            buffer.Lock(&mut data, None, Some(&mut length))?;
            let pcm = std::slice::from_raw_parts(data.cast::<i16>(), length as usize / 2);
            mono.extend(pcm.chunks(channels).map(|f| f[0] as f32 / i16::MAX as f32));
            buffer.Unlock()?;
        }
        let start = start.unwrap_or(0.0);

        // Goertzel power at 1 kHz over 2 ms windows.
        let window = (rate * 0.002) as usize;
        let coefficient = 2.0 * (2.0 * std::f64::consts::PI * TONE_HZ / rate).cos();
        let mut onsets = Vec::new();
        let mut quiet_since = 0.0f64;
        let mut active = false;
        for (index, chunk) in mono.chunks(window).enumerate() {
            let (mut s1, mut s2) = (0.0f64, 0.0f64);
            for &x in chunk {
                let s = x as f64 + coefficient * s1 - s2;
                s2 = s1;
                s1 = s;
            }
            let power = (s1 * s1 + s2 * s2 - coefficient * s1 * s2).sqrt() / chunk.len() as f64 * 2.0;
            let t = start + (index * window) as f64 / rate;
            if power > 0.05 {
                if !active && t - quiet_since >= 0.5 {
                    // Refine to the first sample above a tenth of the burst's amplitude in this or the previous window.
                    let from = (index.saturating_sub(1)) * window;
                    let to = ((index + 1) * window).min(mono.len());
                    let first = (from..to).find(|&i| mono[i].abs() > 0.05).unwrap_or(index * window);
                    onsets.push(start + first as f64 / rate);
                }
                active = true;
            } else {
                if active {
                    quiet_since = t;
                }
                active = false;
            }
        }
        eprintln!("audio seconds decoded: {:.1}", mono.len() as f64 / rate);
        Ok(onsets)
    }
}

// ---------------------------------------------------------------- tracks

/// The boxes in `data` as (type, body).
fn boxes(data: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut found = Vec::new();
    let mut at = 0usize;
    while at + 8 <= data.len() {
        let mut size = u32::from_be_bytes(data[at..at + 4].try_into().unwrap()) as u64;
        let kind: [u8; 4] = data[at + 4..at + 8].try_into().unwrap();
        let mut header = 8u64;
        if size == 1 && at + 16 <= data.len() {
            size = u64::from_be_bytes(data[at + 8..at + 16].try_into().unwrap());
            header = 16;
        } else if size == 0 {
            size = (data.len() - at) as u64;
        }
        if size < header || at as u64 + size > data.len() as u64 {
            break;
        }
        found.push((kind, &data[at + header as usize..at + size as usize]));
        at += size as usize;
    }
    found
}

fn child<'a>(data: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(data).into_iter().find(|(k, _)| k == kind).map(|(_, body)| body)
}

fn be32(data: &[u8], at: usize) -> u64 {
    data.get(at..at + 4)
        .map_or(0, |b| u32::from_be_bytes(b.try_into().unwrap()) as u64)
}

fn be64(data: &[u8], at: usize) -> u64 {
    data.get(at..at + 8)
        .map_or(0, |b| u64::from_be_bytes(b.try_into().unwrap()))
}

/// The timescale and duration of an `mvhd` or `mdhd` body.
fn scale_and_duration(header: &[u8]) -> (u64, u64) {
    if header.first() == Some(&1) {
        (be32(header, 20), be64(header, 24))
    } else {
        (be32(header, 12), be32(header, 16))
    }
}

/// How many of a track's first sample sizes identify it among the streams Media Foundation lists.
const SIGNATURE: usize = 12;

/// A track as the file stores it.
struct Track {
    id: u64,
    handler: String,
    sizes: Vec<u64>,
}

/// Each track's first sample sizes from a `moof` body, added to `sizes` by track ID.
fn fragment_sizes(moof: &[u8], defaults: &[(u64, u64)], sizes: &mut [Track]) {
    for (kind, traf) in boxes(moof) {
        if &kind != b"traf" {
            continue;
        }
        let tfhd = child(traf, b"tfhd").unwrap_or_default();
        let flags = be32(tfhd, 0) & 0xFF_FFFF;
        let id = be32(tfhd, 4);
        let mut at = 8;
        for (flag, length) in [(0x1, 8), (0x2, 4), (0x8, 4)] {
            if flags & flag != 0 {
                at += length;
            }
        }
        let default = if flags & 0x10 != 0 {
            be32(tfhd, at)
        } else {
            defaults
                .iter()
                .find(|(track, _)| *track == id)
                .map_or(0, |(_, size)| *size)
        };
        let Some(entry) = sizes.iter_mut().find(|track| track.id == id) else {
            continue;
        };
        for (kind, trun) in boxes(traf) {
            if &kind != b"trun" {
                continue;
            }
            let flags = be32(trun, 0) & 0xFF_FFFF;
            let count = be32(trun, 4);
            let mut at = 8 + if flags & 0x1 != 0 { 4 } else { 0 } + if flags & 0x4 != 0 { 4 } else { 0 };
            for _ in 0..count {
                if flags & 0x100 != 0 {
                    at += 4;
                }
                let size = if flags & 0x200 != 0 {
                    at += 4;
                    be32(trun, at - 4)
                } else {
                    default
                };
                for flag in [0x400, 0x800] {
                    if flags & flag != 0 {
                        at += 4;
                    }
                }
                if entry.sizes.len() < SIGNATURE {
                    entry.sizes.push(size);
                }
            }
        }
    }
}

/// Prints the top-level layout, each track in stored order and the QuickTime metadata keys. Returns each track's ID
/// with its handler and its first sample sizes.
fn structure(path: &str) -> anyhow::Result<Vec<Track>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    let length = file.metadata()?.len();
    let mut at = 0u64;
    let (mut fragments, mut moov, mut layout) = (0usize, None, Vec::new());
    let mut moofs = Vec::new();
    while at + 8 <= length {
        let mut header = [0u8; 16];
        file.seek(SeekFrom::Start(at))?;
        file.read_exact(&mut header[..8])?;
        let mut size = u32::from_be_bytes(header[..4].try_into()?) as u64;
        let kind: [u8; 4] = header[4..8].try_into()?;
        let mut header_size = 8;
        if size == 1 {
            file.read_exact(&mut header[8..16])?;
            size = u64::from_be_bytes(header[8..16].try_into()?);
            header_size = 16;
        } else if size == 0 {
            size = length - at;
        }
        if size < header_size {
            bail!("a box at {at} is shorter than its header");
        }
        if &kind == b"moof" {
            fragments += 1;
        } else {
            layout.push(String::from_utf8_lossy(&kind).into_owned());
        }
        if &kind == b"moov" || (&kind == b"moof" && moofs.len() < 64) {
            let mut body = vec![0u8; (size - header_size) as usize];
            file.seek(SeekFrom::Start(at + header_size))?;
            file.read_exact(&mut body)?;
            if &kind == b"moov" {
                moov = Some(body);
            } else {
                moofs.push(body);
            }
        }
        at += size;
    }
    println!(
        "file: {length} bytes; top-level boxes other than moof: {}; moof: {fragments}",
        layout.join(" ")
    );
    let moov = moov.context("the file has no moov")?;
    if let Some(mvhd) = child(&moov, b"mvhd") {
        let (scale, duration) = scale_and_duration(mvhd);
        println!("movie: {:.3} s in its header", duration as f64 / scale.max(1) as f64);
    }
    let mut position = 0;
    let mut sizes = Vec::new();
    for (kind, trak) in boxes(&moov) {
        if &kind != b"trak" {
            continue;
        }
        position += 1;
        let tkhd = child(trak, b"tkhd").unwrap_or_default();
        let id = if tkhd.first() == Some(&1) {
            be32(tkhd, 20)
        } else {
            be32(tkhd, 12)
        };
        let mdia = child(trak, b"mdia").unwrap_or_default();
        let handler = child(mdia, b"hdlr")
            .and_then(|h| h.get(8..12))
            .map(String::from_utf8_lossy)
            .unwrap_or_default();
        let (scale, duration) = scale_and_duration(child(mdia, b"mdhd").unwrap_or_default());
        let stbl = child(mdia, b"minf").and_then(|m| child(m, b"stbl")).unwrap_or_default();
        let codec = child(stbl, b"stsd")
            .and_then(|s| s.get(12..16))
            .map(String::from_utf8_lossy)
            .unwrap_or_default();
        let stsz = child(stbl, b"stsz").unwrap_or_default();
        let (fixed, count) = (be32(stsz, 4), be32(stsz, 8));
        let table: Vec<u64> = (0..count.min(SIGNATURE as u64) as usize)
            .map(|index| if fixed != 0 { fixed } else { be32(stsz, 12 + 4 * index) })
            .collect();
        println!(
            "track {position}: id {id}, {handler} {codec}, {:.3} s in its header, {count} samples in its table",
            duration as f64 / scale.max(1) as f64
        );
        sizes.push(Track {
            id,
            handler: handler.into_owned(),
            sizes: table,
        });
    }
    let defaults: Vec<(u64, u64)> = child(&moov, b"mvex")
        .map(|mvex| {
            boxes(mvex)
                .into_iter()
                .filter(|(kind, _)| kind == b"trex")
                .map(|(_, trex)| (be32(trex, 4), be32(trex, 16)))
                .collect()
        })
        .unwrap_or_default();
    for moof in &moofs {
        fragment_sizes(moof, &defaults, &mut sizes);
    }
    for (kind, meta) in boxes(&moov) {
        if &kind != b"meta" {
            continue;
        }
        let (Some(keys), Some(items)) = (child(meta, b"keys"), child(meta, b"ilst")) else {
            continue;
        };
        let names: Vec<String> = boxes(keys.get(8..).unwrap_or_default())
            .into_iter()
            .map(|(_, key)| String::from_utf8_lossy(key).into_owned())
            .collect();
        for (index, item) in boxes(items) {
            let index = u32::from_be_bytes(index) as usize;
            let value = child(item, b"data")
                .and_then(|d| d.get(8..))
                .map(String::from_utf8_lossy)
                .unwrap_or_default();
            let name = names.get(index.wrapping_sub(1)).map_or("?", String::as_str);
            println!("metadata: {name} = {value}");
        }
    }
    Ok(sizes)
}

/// Media Foundation does not list a file's streams in the order of its tracks, and its stream identifiers are not the
/// track IDs, so each stream is matched to the track whose first sample sizes it reads.
unsafe fn track_of_stream(path: &str, index: u32, handler: &str, tracks: &[Track]) -> anyhow::Result<String> {
    unsafe {
        let reader = reader(path)?;
        reader.SetStreamSelection(0xFFFF_FFFE, false)?;
        reader.SetStreamSelection(index, true)?;
        let mut read = Vec::new();
        while read.len() < SIGNATURE {
            let mut flags = 0u32;
            let mut sample = None;
            reader.ReadSample(index, 0, None, Some(&mut flags), None, Some(&mut sample))?;
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                break;
            }
            if let Some(sample) = sample {
                read.push(sample.GetTotalLength()? as u64);
            }
        }
        let name = |(position, track): (usize, &Track)| format!("track {} (id {})", position + 1, track.id);
        let mut found: Vec<String> = tracks
            .iter()
            .enumerate()
            .filter(|(_, track)| !track.sizes.is_empty() && track.sizes == read)
            .map(name)
            .collect();
        // A decoder may be handed a video sample with its parameter sets added, so the only track of a kind is its own.
        let of_kind: Vec<(usize, &Track)> = tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.handler == handler)
            .collect();
        if found.is_empty() && of_kind.len() == 1 {
            found.push(format!("{}, the only {handler} track", name(of_kind[0])));
        }
        Ok(match found.as_slice() {
            [one] => one.clone(),
            [] => "no track with the same sample sizes".to_owned(),
            _ => format!("one of {}", found.join(", ")),
        })
    }
}

fn tracks(path: &str) -> anyhow::Result<()> {
    let sizes = structure(path)?;
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        MFStartup(MF_VERSION, MFSTARTUP_FULL)?;
        let reader = reader(path)?;
        let mut index = 0;
        while let Ok(native) = reader.GetNativeMediaType(index, 0) {
            let audio = native.GetGUID(&MF_MT_MAJOR_TYPE)? == MFMediaType_Audio;
            let track = track_of_stream(path, index, if audio { "soun" } else { "vide" }, &sizes)?;
            if audio {
                decode_audio(path, index, &track)?;
            } else {
                let size = native.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or(0);
                println!("stream {index}, {track}: video {}x{}", size >> 32, size & 0xFFFF_FFFF);
                let samples = decode_video(path, index)?;
                println!("  {samples} samples");
            }
            index += 1;
        }
        Ok(())
    }
}

/// Decodes one video stream and prints its sample count and where in the picture something other than black is,
/// each time that area changes: a window recorded at its own size shows as a box in the middle, and a black picture
/// as none.
unsafe fn decode_video(path: &str, index: u32) -> anyhow::Result<u64> {
    unsafe {
        let reader = reader(path)?;
        reader.SetStreamSelection(0xFFFF_FFFE, false)?;
        reader.SetStreamSelection(index, true)?;
        let media_type = MFCreateMediaType()?;
        media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        media_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
        reader.SetCurrentMediaType(index, None, &media_type)?;
        let mut count = 0;
        let mut last: Option<Option<(u32, u32, u32, u32)>> = None;
        loop {
            let mut flags = 0u32;
            let mut time = 0i64;
            let mut sample = None;
            reader.ReadSample(index, 0, None, Some(&mut flags), Some(&mut time), Some(&mut sample))?;
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                return Ok(count);
            }
            let Some(sample) = sample else { continue };
            count += 1;
            let size = reader.GetCurrentMediaType(index)?.GetUINT64(&MF_MT_FRAME_SIZE)?;
            let (width, height) = ((size >> 32) as u32, (size & 0xFFFF_FFFF) as u32);
            let buffer = sample.ConvertToContiguousBuffer()?;
            let buffer2d: IMF2DBuffer = buffer.cast()?;
            let mut scan0 = std::ptr::null_mut();
            let mut pitch = 0i32;
            buffer2d.Lock2D(&mut scan0, &mut pitch)?;
            // The box around every luma sample brighter than black, on a grid of 4 pixels.
            let mut found: Option<(u32, u32, u32, u32)> = None;
            for y in (0..height).step_by(4) {
                let line = scan0.offset(y as isize * pitch as isize);
                for x in (0..width).step_by(4) {
                    if *line.add(x as usize) > 24 {
                        found = Some(match found {
                            None => (x, y, x, y),
                            Some((l, t, r, b)) => (l.min(x), t.min(y), r.max(x), b.max(y)),
                        });
                    }
                }
            }
            buffer2d.Unlock2D()?;
            let moved = match (last, found) {
                (None, _) => true,
                (Some(None), None) => false,
                (Some(Some(a)), Some(b)) => [
                    a.0.abs_diff(b.0),
                    a.1.abs_diff(b.1),
                    a.2.abs_diff(b.2),
                    a.3.abs_diff(b.3),
                ]
                .iter()
                .any(|d| *d > 16),
                _ => true,
            };
            if moved {
                let at = time as f64 / 1e7;
                match found {
                    None => println!("  {at:8.3} s: black ({width}x{height})"),
                    Some((l, t, r, b)) => println!(
                        "  {at:8.3} s: picture in x {l}..{} y {t}..{} ({}x{} of {width}x{height})",
                        r + 4,
                        b + 4,
                        r + 4 - l,
                        b + 4 - t
                    ),
                }
                last = Some(found);
            }
        }
    }
}

/// Decodes one audio stream and prints, per channel, its peak, its mean level and when it first goes above -40 dBFS.
unsafe fn decode_audio(path: &str, index: u32, track: &str) -> anyhow::Result<()> {
    unsafe {
        let reader = reader(path)?;
        reader.SetStreamSelection(0xFFFF_FFFE, false)?;
        reader.SetStreamSelection(index, true)?;
        let media_type = MFCreateMediaType()?;
        media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        media_type.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
        media_type.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        reader.SetCurrentMediaType(index, None, &media_type)?;
        let current = reader.GetCurrentMediaType(index)?;
        let rate = current.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)? as f64;
        let channels = current.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)? as usize;
        let mut start = None;
        let mut frames = 0usize;
        let mut peak = vec![0i32; channels];
        let mut energy = vec![0f64; channels];
        let mut loud = vec![None; channels];
        let threshold = (i16::MAX as f64 * 0.01) as i32; // -40 dBFS
        loop {
            let mut flags = 0u32;
            let mut time = 0i64;
            let mut sample = None;
            reader.ReadSample(index, 0, None, Some(&mut flags), Some(&mut time), Some(&mut sample))?;
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                break;
            }
            let Some(sample) = sample else { continue };
            let at = time as f64 / 1e7;
            start.get_or_insert(at);
            let buffer = sample.ConvertToContiguousBuffer()?;
            let mut data = std::ptr::null_mut();
            let mut length = 0u32;
            buffer.Lock(&mut data, None, Some(&mut length))?;
            let pcm = std::slice::from_raw_parts(data.cast::<i16>(), length as usize / 2);
            for (offset, frame) in pcm.chunks(channels).enumerate() {
                for (channel, &value) in frame.iter().enumerate() {
                    let value = value as i32;
                    peak[channel] = peak[channel].max(value.abs());
                    energy[channel] += (value as f64).powi(2);
                    if loud[channel].is_none() && value.abs() > threshold {
                        loud[channel] = Some(at + offset as f64 / rate);
                    }
                }
            }
            frames += pcm.len() / channels;
            buffer.Unlock()?;
        }
        println!(
            "stream {index}, {track}: audio {channels} ch at {rate} Hz, first sample at {:.3} s, {:.3} s decoded",
            start.unwrap_or(0.0),
            frames as f64 / rate
        );
        for channel in 0..channels {
            let rms = (energy[channel] / frames.max(1) as f64).sqrt() / i16::MAX as f64;
            println!(
                "  channel {channel}: peak {} ({:.1} dBFS), mean {:.1} dBFS, above -40 dBFS from {}",
                peak[channel],
                20.0 * (peak[channel].max(1) as f64 / i16::MAX as f64).log10(),
                20.0 * rms.max(1e-6).log10(),
                loud[channel].map_or("never".to_owned(), |t| format!("{t:.3} s"))
            );
        }
        Ok(())
    }
}
