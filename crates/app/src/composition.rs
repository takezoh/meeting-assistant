//! Where the real implementations are wired together.
//!
//! The decision-making side (core, `worker`, `linked`) knows only the ports.
//! Here we assemble them by supplying the OS implementation (`platform`), the browser extension connection (`browser`),
//! and the clock and storage (`adapters`).

use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use meeting_assistant_core::{Config, Engine, Notice, NoticeLevel, RecordingCoordinator};

use crate::adapters::{DiskFiles, Mp4Remuxer, real_ports};
use crate::browser::BrowserLinkServer;
use crate::linked::{LinkedRecorder, LinkedSignalSource};
use crate::platform::{self, PlatformLoopbackPeers, PlatformRecorder, PlatformSignalSource};
use crate::recording::merge::Merger;
use crate::worker::{self, Command, Update};

/// Starts the thread that runs detection and recording.
/// `publish` is called on every state change and every notification.
pub fn spawn_engine(
    config: Config,
    output_directory: PathBuf,
    publish: impl Fn(Update) + Send + 'static,
) -> std::io::Result<mpsc::Sender<Command>> {
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new().name("engine".into()).spawn(move || {
        // Some OS signal sources are bound to a thread, so they are built inside this thread.
        match build_engine(&config, output_directory) {
            Ok(engine) => {
                let interval = Duration::from_millis(config.poll_interval_milliseconds);
                worker::run(engine, interval, &receiver, publish);
            }
            Err(error) => {
                log::error!("cannot start detection: {error:#}");
                publish(Update::Notice(Notice {
                    level: NoticeLevel::Error,
                    message: "Cannot start meeting detection. Check the log.".into(),
                }));
            }
        }
    })?;
    Ok(sender)
}

type AppEngine = Engine<
    LinkedSignalSource<PlatformSignalSource, BrowserLinkServer>,
    LinkedRecorder<PlatformRecorder, BrowserLinkServer, PlatformRecorder, Merger<Mp4Remuxer, DiskFiles>>,
>;

fn build_engine(config: &Config, output_directory: PathBuf) -> anyhow::Result<AppEngine> {
    // Even without the browser extension, the other detection and recording keep working.
    let browser = BrowserLinkServer::start(Arc::new(PlatformLoopbackPeers))
        .inspect_err(|error| log::warn!("not linking with the browser extension: {error:#}"))
        .ok();

    let recorder = LinkedRecorder::new(
        platform::recorder(config.video.clone(), config.audio.clone()),
        browser.clone(),
        // A second recorder records the microphone beside a browser tab recording.
        platform::recorder(config.video.clone(), config.audio.clone()),
        Merger::new(Mp4Remuxer, DiskFiles),
    );
    let coordinator =
        RecordingCoordinator::new(recorder, real_ports(output_directory)).with_capture_scope(config.capture_scope);
    let signals = LinkedSignalSource::new(PlatformSignalSource::new()?, browser);
    Ok(Engine::new(signals, coordinator, config)?)
}
