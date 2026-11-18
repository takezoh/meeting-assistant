//! Diagnostic features run from the command line.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use meeting_assistant_core::{
    AudioSession, Config, Recorder, RecordingRequest, RuleEvaluator, SignalSource, VideoTarget,
};

use crate::platform::{self, PlatformSignalSource};

/// Shows the current signals and the verdict of each rule. Used for tuning rules.
pub fn probe(config: &Config) -> anyhow::Result<()> {
    let snapshot = PlatformSignalSource::new()?.capture()?;
    let names: HashMap<u32, &str> = snapshot
        .processes
        .iter()
        .map(|p| (p.process_id, p.name.as_str()))
        .collect();
    let list = |title: &str, sessions: &[AudioSession]| {
        println!("{title}:");
        for session in sessions {
            println!(
                "  {} (pid {})",
                names.get(&session.process_id).unwrap_or(&"?"),
                session.process_id
            );
        }
        if sessions.is_empty() {
            println!("  (none)");
        }
    };
    list("Processes using the microphone", &snapshot.capture_sessions);
    list("Processes playing audio", &snapshot.render_sessions);
    println!(
        "Processes: {} / windows: {}",
        snapshot.processes.len(),
        snapshot.windows.len()
    );

    let mark = |value: bool| if value { "yes" } else { "no" };
    println!("Verdict per rule:");
    for evidence in RuleEvaluator::new(&config.rules)?.evaluate(&snapshot) {
        let process = evidence
            .process_id
            .map(|p| format!(" audio target=pid {p}"))
            .unwrap_or_default();
        let window = evidence.window.map(|w| format!(" window={w:#x}")).unwrap_or_default();
        let display = evidence.display.map(|d| format!(" display={d}")).unwrap_or_default();
        println!(
            "  {}: active={}{process}{window}{display}",
            evidence.rule_name,
            mark(evidence.active),
        );
    }
    Ok(())
}

/// Records the main display and the default audio devices for `length` and checks that a file is produced.
/// If `keep` is given the file is kept there; otherwise it is deleted.
pub fn self_test(config: &Config, keep: Option<PathBuf>, length: Duration) -> anyhow::Result<()> {
    let output_path = keep
        .clone()
        .unwrap_or_else(|| std::env::temp_dir().join(format!("meeting-assistant-selftest-{}.mp4", std::process::id())));
    let mut recorder = platform::recorder(config.video.clone(), config.audio.clone());
    let request = RecordingRequest {
        output_path: output_path.clone(),
        video: VideoTarget::Display(None),
        audio_process_id: None,
        render_device_id: None,
        capture_device_id: None,
    };

    recorder.start(&request).context("cannot start recording")?;
    println!("Recording started. Recording for {} seconds.", length.as_secs());
    std::thread::sleep(length);
    let failure = recorder.take_failure();
    let stopped = recorder.stop();

    let size = std::fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);
    if keep.is_none() {
        let _ = std::fs::remove_file(&output_path);
    }
    if let Some(failure) = failure {
        anyhow::bail!("recording was interrupted: {failure}");
    }
    stopped.context("cannot stop recording")?;
    anyhow::ensure!(size > 0, "no recording file was created");
    println!("Success: created a recording file of {size} bytes.");
    Ok(())
}
