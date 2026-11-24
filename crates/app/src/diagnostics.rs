//! Diagnostic features run from the command line.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use meeting_assistant_core::{
    AudioSession, Config, Recorder, RecordingMetadata, RecordingRequest, RuleEvaluator, Sound, VideoTarget,
    full_snapshot,
};

use crate::platform::{self, PlatformSignalSource};

/// Shows the current signals and the verdict of each rule. Used for tuning rules.
pub fn probe(config: &Config) -> anyhow::Result<()> {
    let snapshot = full_snapshot(&mut PlatformSignalSource::new()?)?;
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
    let evaluator = RuleEvaluator::new(&config.rules)?.with_apps(&crate::apps::adapters());
    for evidence in evaluator.evaluate(&snapshot) {
        let process = evidence
            .process_id
            .map(|p| format!(" audio target=pid {p}"))
            .unwrap_or_default();
        // Of a window the app's adapter tells nothing about, whether it is the meeting's depends on when it appeared,
        // which a single reading cannot tell.
        let roles: Vec<String> = evidence
            .windows
            .iter()
            .map(|w| {
                let told: Vec<String> = w.metadata.entries().iter().map(|(k, v)| format!("{k}={v:?}")).collect();
                if told.is_empty() {
                    format!("{:#x}:{:?}", w.id, w.role)
                } else {
                    format!("{:#x}:{:?}({})", w.id, w.role, told.join(","))
                }
            })
            .collect();
        let windows = format!(" windows=[{}]", roles.join(" "));
        println!(
            "  {}: active={}{process}{windows}",
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
        microphone: true,
        // A self-test records through the platform alone, which writes no metadata into the file.
        metadata: RecordingMetadata {
            app: "SelfTest".into(),
            ..RecordingMetadata::default()
        },
    };

    let started = recorder.start(&request).context("cannot start recording")?;
    println!("Recording started. Recording for {} seconds.", length.as_secs());
    for missing in &started.missing {
        let track = match missing.sound {
            Sound::OtherSide => "other side",
            Sound::Microphone => "microphone",
        };
        println!("Warning: the {track} track stays silent: {}", missing.reason);
    }
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
