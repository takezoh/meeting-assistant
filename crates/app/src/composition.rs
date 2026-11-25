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
use crate::browser::protocol::PictureSize;
use crate::linked::{LinkedRecorder, LinkedSignalSource};
use crate::platform::{self, PlatformLoopbackPeers, PlatformRecorder, PlatformSignalSource};
use crate::recording::annotate::Annotator;
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
    LinkedRecorder<
        PlatformRecorder,
        BrowserLinkServer,
        PlatformRecorder,
        Merger<Mp4Remuxer, DiskFiles>,
        Annotator<Mp4Remuxer, DiskFiles>,
    >,
>;

fn build_engine(config: &Config, output_directory: PathBuf) -> anyhow::Result<AppEngine> {
    // Even without the browser extension, the other detection and recording keep working.
    // A tab is recorded at most at the size of every other recording's picture.
    let largest_picture = PictureSize {
        width: config.video.width,
        height: config.video.height,
    };
    let browser = BrowserLinkServer::start(Arc::new(PlatformLoopbackPeers), largest_picture)
        .inspect_err(|error| log::warn!("not linking with the browser extension: {error:#}"))
        .ok();

    let recorder = LinkedRecorder::new(
        platform::recorder(config.video.clone(), config.audio.clone()),
        browser.clone(),
        // A second recorder records the microphone beside a browser tab recording.
        platform::recorder(config.video.clone(), config.audio.clone()),
        Merger::new(Mp4Remuxer, DiskFiles),
        // Every finished recording gets what is known of it written into its file.
        Annotator::new(Mp4Remuxer, DiskFiles),
    );
    let mut coordinator =
        RecordingCoordinator::new(recorder, real_ports(output_directory)).with_capture_scope(config.capture_scope);
    // What a run that was killed left in progress goes to the recordings folder before anything new is recorded.
    coordinator.recover_unfinished();
    let signals = LinkedSignalSource::new(PlatformSignalSource::new()?, browser);
    Ok(Engine::new(signals, coordinator, config)?
        .with_apps(&crate::apps::adapters())
        .with_own_process(std::process::id()))
}
