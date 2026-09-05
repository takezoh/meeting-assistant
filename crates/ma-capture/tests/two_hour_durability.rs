//! contract-two-hour-durability: the portable proof of the two-hour window. A synthetic source is
//! driven for exactly two hours of samples through the unchanged chunk writer; the manifest, the
//! chunk directory and the produced sample count must agree, with no gap. The real two-hour run
//! against a target application is the Windows-tier manual observation `v-win1-two-hour-live`.

use ma_capture::*;
use ma_core_types::id::TypedId;
use ma_core_types::TrackId;
use std::path::Path;

/// Two hours at the writer's pinned rate.
const TWO_HOURS_SAMPLES: u64 = 2 * 60 * 60 * SAMPLE_RATE as u64;

fn wav_files(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".wav"))
        .collect();
    v.sort();
    v
}

#[test]
fn two_hour_chunk_accounting_from_synthetic_source() {
    assert_eq!(TWO_HOURS_SAMPLES, 115_200_000);
    let dir = tempfile::tempdir().unwrap();
    let track = dir.path().join("loopback");
    // One-second blocks, as a live source would deliver them; no wall-clock dependence.
    let mut source = SyntheticSource::new(SAMPLE_RATE, TWO_HOURS_SAMPLES, SAMPLE_RATE as usize);
    let mut writer =
        ChunkWriter::open(&track, TrackId::new(), "loopback", source.origin()).unwrap();
    let mut fs = RealFs;
    loop {
        match source.next() {
            SourceEvent::Samples(s) => {
                writer.push(&s);
                writer.drain(&mut fs).unwrap();
                assert!(
                    writer.queued_samples() <= QUEUE_CAP_SAMPLES,
                    "the queue never exceeds its loss window"
                );
            }
            SourceEvent::FormatChanged(_) => panic!("a synthetic source does not change format"),
            SourceEvent::Ended => break,
        }
    }
    writer.finish(&mut fs).unwrap();
    assert_eq!(source.produced(), TWO_HOURS_SAMPLES);

    let expected_chunks = (TWO_HOURS_SAMPLES / CHUNK_SAMPLES as u64) as usize;
    assert_eq!(expected_chunks, 240, "thirty-second chunks over two hours");
    let manifest = writer.manifest();
    assert_eq!(
        manifest.chunks.len(),
        expected_chunks,
        "the manifest names every chunk"
    );
    assert!(
        manifest.gaps.is_empty(),
        "no gap record over the whole window"
    );
    let total: u64 = manifest.chunks.iter().map(|c| c.len_samples).sum();
    assert_eq!(
        total, TWO_HOURS_SAMPLES,
        "every produced sample is accounted for"
    );
    assert_eq!(writer.position(), TWO_HOURS_SAMPLES);
    // Chunks are dense from zero, thirty seconds each.
    for (i, chunk) in manifest.chunks.iter().enumerate() {
        assert_eq!(chunk.seq as usize, i);
        assert_eq!(chunk.start_sample, i as u64 * CHUNK_SAMPLES as u64);
        assert_eq!(chunk.len_samples, CHUNK_SAMPLES as u64);
    }
    // The directory is the truth: exactly the manifest's files, nothing partial left behind.
    let files = wav_files(&track);
    assert_eq!(files.len(), expected_chunks);
    assert_eq!(files.first().map(String::as_str), Some("000000.wav"));
    assert_eq!(files.last().map(String::as_str), Some("000239.wav"));
    assert!(
        std::fs::read_dir(&track)
            .unwrap()
            .flatten()
            .all(|e| !e.file_name().to_string_lossy().ends_with(".part")),
        "no .part file remains after finish"
    );
    // The saved manifest agrees with the in-memory one and with the directory.
    let saved = ChunkManifest::load(&track)
        .unwrap()
        .expect("manifest saved");
    assert_eq!(saved.chunks.len(), expected_chunks);
    assert_eq!(saved.origin.sample_rate, SAMPLE_RATE);
    let report = recover(&track, 1).unwrap();
    assert!(
        report.gapped.is_empty() && report.adopted.is_empty() && report.repaired.is_empty(),
        "recovery over a clean two-hour track changes nothing: {report:?}"
    );
}
