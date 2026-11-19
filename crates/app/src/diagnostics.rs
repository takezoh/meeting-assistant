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

    // The windows of processes that a rule names, or their descendants: what a rule can choose from. Other apps'
    // windows are left out, since their titles may show anything.
    let parents: HashMap<u32, u32> = snapshot
        .processes
        .iter()
        .map(|p| (p.process_id, p.parent_process_id))
        .collect();
    let rule_names: Vec<String> = config
        .rules
        .iter()
        .flat_map(|rule| rule.process_names.iter().map(|name| name.to_lowercase()))
        .collect();
    let named_by_rule = |mut pid: u32| {
        for _ in 0..32 {
            let name = names.get(&pid).map(|name| name.to_lowercase());
            if name.is_some_and(|name| rule_names.iter().any(|rule| name.trim_end_matches(".exe") == *rule)) {
                return true;
            }
            match parents.get(&pid) {
                Some(&parent) if parent != pid && parent != 0 => pid = parent,
                _ => return false,
            }
        }
        false
    };
    println!("Windows of meeting apps:");
    let mut shown = 0;
    for window in snapshot.windows.iter().filter(|w| named_by_rule(w.process_id)) {
        println!(
            "  {:#x} {} (pid {}) area={} display={} title={:?}",
            window.id,
            names.get(&window.process_id).unwrap_or(&"?"),
            window.process_id,
            window.area,
            window.display.as_deref().unwrap_or("?"),
            window.title,
        );
        shown += 1;
    }
    if shown == 0 {
        println!("  (none)");
    }

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

/// Records the main display and the default audio device for 5 seconds and checks that a file is produced.
/// If `keep` is given the file is kept there; otherwise it is deleted.
pub fn self_test(config: &Config, keep: Option<PathBuf>) -> anyhow::Result<()> {
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
    println!("Recording started. Recording for 5 seconds.");
    std::thread::sleep(Duration::from_secs(5));
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
