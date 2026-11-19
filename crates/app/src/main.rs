// Stay resident without showing a console window. Command-line output is handled by platform::has_output.
#![cfg_attr(windows, windows_subsystem = "windows")]

//! Entry point of the executable. It only parses the arguments and dispatches to each action; it makes no decisions.
//!
//! | Module | Responsibility | Side effects |
//! |---|---|---|
//! | `cli` | Argument parsing | None |
//! | `install` | Installation steps | None (OS operations go through ports) |
//! | `recording` | Recording time and audio calculations | None |
//! | `browser` | Browser extension link | None for protocol and hub; network and files for server and relay |
//! | `linked` | Choosing whether to detect and record via the OS or the extension | None (goes through ports) |
//! | `worker` | Loop that advances detection periodically and handles instructions | Only the passage of time |
//! | `panel::view` | What the status panel and tray icon show, and what the panel's buttons ask for | None |
//! | `adapters` | Implementations of core's ports (clock, storage, config file) | Files, clock |
//! | `platform` | Per-OS implementations | OS APIs |
//! | `composition`, `tray`, `panel::window`, `diagnostics`, `logging` | Wiring and interaction with the user | Yes |

mod adapters;
mod apps;
#[cfg(test)]
mod architecture;
mod browser;
mod bundle;
mod cli;
mod composition;
mod diagnostics;
mod install;
mod linked;
mod logging;
mod panel;
mod platform;
mod recording;
mod tray;
mod worker;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use meeting_assistant_core::Config;

use crate::cli::{Command, Invocation};
use crate::platform::PlatformInstaller;

/// Locations of the config and logs. If a config location is given, logs go to the `logs` folder next to it.
fn locations(config_override: Option<PathBuf>) -> (PathBuf, PathBuf) {
    match config_override {
        Some(config) => {
            let logs = config
                .parent()
                .map(|p| p.join("logs"))
                .unwrap_or_else(|| PathBuf::from("logs"));
            (config, logs)
        }
        None => {
            let config = dirs::config_dir()
                .unwrap_or_default()
                .join("MeetingAssistant")
                .join("config.toml");
            let logs = dirs::data_local_dir()
                .unwrap_or_default()
                .join("MeetingAssistant")
                .join("logs");
            (config, logs)
        }
    }
}

/// Reports the result to the user: to the command-line output if there is one, otherwise in a dialog.
struct Reporter {
    has_output: bool,
}

impl Reporter {
    fn info(&self, message: &str) {
        if self.has_output {
            println!("{message}");
        } else {
            log::info!("{}", message.replace('\n', " / "));
            platform::show_info(message);
        }
    }

    fn error(&self, message: &str) -> ExitCode {
        log::error!("{message}");
        if self.has_output {
            println!("Failed: {message}");
        } else {
            platform::show_error(message);
        }
        ExitCode::FAILURE
    }
}

fn main() -> ExitCode {
    let (config_override, command) = match cli::parse(std::env::args().skip(1)) {
        Ok(Invocation::NativeHost) => return run_native_host(),
        Ok(Invocation::Command { config, command }) => (config, command),
        Err(cli::UsageError) => {
            if platform::has_output() {
                print!("{}", cli::USAGE);
            } else {
                platform::show_info(cli::USAGE);
            }
            return ExitCode::from(2);
        }
    };
    let (config_path, log_directory) = locations(config_override);

    // When running resident, assume there is no output; report why startup failed in a dialog.
    let reporter = Reporter {
        has_output: command != Command::Run && platform::has_output(),
    };
    logging::init(&log_directory, reporter.has_output);

    let result = match command {
        // Install-related commands work even if the config file is missing or broken.
        Command::Install => install::install(&mut PlatformInstaller).map(|report| reporter.info(&report.join("\n"))),
        Command::Uninstall => {
            install::uninstall(&mut PlatformInstaller).map(|report| reporter.info(&report.join("\n")))
        }
        Command::Quit => quit(&reporter),
        Command::Probe => load(&config_path, false).and_then(|config| diagnostics::probe(&config)),
        Command::SelfTest { keep, seconds } => load(&config_path, false)
            .and_then(|config| diagnostics::self_test(&config, keep, Duration::from_secs(seconds))),
        Command::Run => load(&config_path, true).and_then(|config| run_tray(config, config_path, log_directory)),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => reporter.error(&format!("{error:#}")),
    }
}

/// Reads the configuration. A file for another configuration version has been replaced with the defaults by then;
/// that is said in the log and, when `notify` is set (the resident app, which has no console), in a notification.
fn load(config_path: &std::path::Path, notify: bool) -> anyhow::Result<Config> {
    let loaded = adapters::config_file::load_or_create(config_path)?;
    if let Some(replaced) = &loaded.replaced {
        let message = format!(
            "The settings file was written for {}, which this version does not read, so it was replaced with the \
             defaults. The old file is kept at {}.",
            replaced.found.map_or_else(
                || "an earlier version of the app".to_owned(),
                |version| format!("configuration version {version}")
            ),
            replaced.kept_at.display()
        );
        log::warn!("{message}");
        if notify {
            platform::notify(install::APP_NAME, &message);
        }
    }
    Ok(loaded.config)
}

/// Runs as a relay process launched by the browser.
fn run_native_host() -> ExitCode {
    logging::init(&locations(None).1, false);
    match browser::relay::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            log::warn!("cannot start relaying to the extension: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn quit(reporter: &Reporter) -> anyhow::Result<()> {
    anyhow::ensure!(platform::request_quit(), "the resident app did not exit in time");
    reporter.info("No resident app is running (or it has exited).");
    Ok(())
}

fn run_tray(config: Config, config_file: PathBuf, log_directory: PathBuf) -> anyhow::Result<()> {
    let Some(_instance) = platform::SingleInstance::acquire() else {
        anyhow::bail!("Meeting Assistant is already running");
    };
    let output_directory = config
        .output_directory
        .clone()
        .unwrap_or_else(|| dirs::video_dir().unwrap_or_default().join("MeetingAssistant"));

    log::info!("Started. Config: {}", config_file.display());
    log::info!("Recording folder: {}", output_directory.display());
    tray::run(
        config,
        tray::Locations {
            config_file,
            log_directory,
            output_directory,
        },
    )
    .map(|never| match never {})
}
