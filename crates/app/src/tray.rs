//! The notification-area icon and menu.
//! Icon colors: gray = idle, yellow = in a meeting but not recording, red = recording.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Context;
use meeting_assistant_core::{Config, CoordinatorStatus, Notice, NoticeLevel};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::composition::spawn_engine;
use crate::install::{self, InstallSystem};
use crate::platform::{self, PlatformInstaller};
use crate::worker::{Command, Update};

const APP_NAME: &str = "Meeting Assistant";
/// Maximum time to wait for the recording to be finalized on exit.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(60);

enum AppEvent {
    Menu(MenuId),
    Worker(Update),
}

pub struct Locations {
    pub config_file: PathBuf,
    pub log_directory: PathBuf,
    pub output_directory: PathBuf,
}

struct Tray {
    icon: TrayIcon,
    status_item: MenuItem,
    record_item: MenuItem,
    auto_item: CheckMenuItem,
    open_output: MenuItem,
    open_config: MenuItem,
    open_logs: MenuItem,
    autostart_item: CheckMenuItem,
    /// Shown only when launched from somewhere other than the install location.
    install_item: Option<MenuItem>,
    quit: MenuItem,
    idle_icon: Icon,
    meeting_icon: Icon,
    recording_icon: Icon,
    status: Option<CoordinatorStatus>,
}

/// Stays resident. Selecting Quit ends the process, so this returns only if setup fails.
pub fn run(config: Config, locations: Locations) -> anyhow::Result<std::convert::Infallible> {
    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();

    let menu_proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let _ = menu_proxy.send_event(AppEvent::Menu(event.id));
    }));

    let worker_proxy = event_loop.create_proxy();
    let commands = spawn_engine(config, locations.output_directory.clone(), move |update| {
        let _ = worker_proxy.send_event(AppEvent::Worker(update));
    })
    .context("cannot start the detection thread")?;

    // On logoff, shutdown, or a quit request from another process, finalize the recording before exiting.
    platform::watch_exit_requests({
        let commands = commands.clone();
        move || shut_down(&commands)
    });

    // On macOS, stay resident in the menu bar only, with no Dock icon, and do not take the focus at start.
    #[cfg(target_os = "macos")]
    let mut event_loop = event_loop;
    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
        event_loop.set_activate_ignoring_other_apps(false);
    }

    let mut tray: Option<Tray> = None;
    // Several exit paths can reach the end of the loop; the recording is finalized by the first one.
    let mut finalized = false;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            // Create the icon after the event loop has started (required on macOS).
            Event::NewEvents(StartCause::Init) => match Tray::create() {
                Ok(created) => tray = Some(created),
                Err(error) => {
                    log::error!("cannot create the notification-area icon: {error:#}");
                    *control_flow = ControlFlow::ExitWithCode(1);
                }
            },
            Event::UserEvent(AppEvent::Worker(Update::Status(status))) => {
                if let Some(tray) = &mut tray {
                    tray.render(status);
                }
            }
            Event::UserEvent(AppEvent::Worker(Update::Notice(notice))) => show_notice(&notice),
            Event::UserEvent(AppEvent::Menu(id)) => {
                let Some(tray) = &mut tray else { return };
                if id == *tray.record_item.id() {
                    let recording = tray.status.as_ref().is_some_and(CoordinatorStatus::is_recording);
                    let _ = commands.send(if recording {
                        Command::StopManual
                    } else {
                        Command::StartManual
                    });
                } else if id == *tray.auto_item.id() {
                    let _ = commands.send(Command::SetAutoEnabled(tray.auto_item.is_checked()));
                } else if id == *tray.open_output.id() {
                    open_directory(&locations.output_directory);
                } else if id == *tray.open_config.id() {
                    open(&locations.config_file);
                } else if id == *tray.open_logs.id() {
                    open_directory(&locations.log_directory);
                } else if id == *tray.autostart_item.id() {
                    if let Err(error) = install::set_autostart(&mut PlatformInstaller, tray.autostart_item.is_checked())
                    {
                        log::error!("cannot toggle start at logon: {error:#}");
                    }
                    tray.autostart_item.set_checked(PlatformInstaller.autostart_enabled());
                } else if tray.install_item.as_ref().is_some_and(|item| id == *item.id()) {
                    // Installation runs in a separate process, which asks this process to quit before proceeding.
                    let started = std::env::current_exe()
                        .and_then(|executable| std::process::Command::new(executable).arg("--install").spawn());
                    if let Err(error) = started {
                        log::error!("cannot start the installation: {error}");
                    }
                } else if id == *tray.quit.id() {
                    let _ = tray.icon.set_tooltip(Some(format!("{APP_NAME} - Quitting")));
                    finalized = true;
                    shut_down(&commands);
                    *control_flow = ControlFlow::Exit;
                }
            }
            // The last event before the process ends. Besides a quit from the menu, it is how the end of the
            // session reaches a macOS app: logout, restart, shutdown and a quit request from another app arrive
            // as a request to terminate the application, and the process exits right after this event.
            Event::LoopDestroyed if !std::mem::replace(&mut finalized, true) => {
                log::info!("The application is being terminated.");
                shut_down(&commands);
            }
            _ => {}
        }
    })
}

/// Finalizes the recording and stops detection. Waits until done (up to the limit).
fn shut_down(commands: &mpsc::Sender<Command>) {
    let (done_sender, done_receiver) = mpsc::channel();
    if commands.send(Command::Shutdown(done_sender)).is_ok() && done_receiver.recv_timeout(SHUTDOWN_TIMEOUT).is_err() {
        log::warn!("could not finalize the recording in time; the file can still be played up to where it stopped");
    }
    log::info!("Exited.");
}

impl Tray {
    fn create() -> anyhow::Result<Self> {
        let status_item = MenuItem::new("Idle", false, None);
        let record_item = MenuItem::new("Start recording now", true, None);
        let auto_item = CheckMenuItem::new("Record meetings automatically", true, true, None);
        let open_output = MenuItem::new("Open recordings folder", true, None);
        let open_config = MenuItem::new("Open config file", true, None);
        let open_logs = MenuItem::new("Open logs folder", true, None);
        let autostart_item = CheckMenuItem::new("Start at logon", true, PlatformInstaller.autostart_enabled(), None);
        let install_item = (!install::is_installed(&PlatformInstaller))
            .then(|| MenuItem::new("Install (set up autostart and browser link)", true, None));
        let quit = MenuItem::new("Quit", true, None);

        let menu = Menu::new();
        menu.append_items(&[
            &status_item,
            &PredefinedMenuItem::separator(),
            &record_item,
            &auto_item,
            &PredefinedMenuItem::separator(),
            &open_output,
            &open_config,
            &open_logs,
            &PredefinedMenuItem::separator(),
            &autostart_item,
        ])?;
        if let Some(install_item) = &install_item {
            menu.append(install_item)?;
        }
        menu.append_items(&[&PredefinedMenuItem::separator(), &quit])?;

        let idle_icon = circle_icon([140, 140, 140])?;
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(format!("{APP_NAME} - Idle"))
            .with_icon(idle_icon.clone())
            .build()?;

        Ok(Self {
            icon,
            status_item,
            record_item,
            auto_item,
            open_output,
            open_config,
            open_logs,
            autostart_item,
            install_item,
            quit,
            idle_icon,
            meeting_icon: circle_icon([230, 170, 0])?,
            recording_icon: circle_icon([215, 40, 40])?,
            status: None,
        })
    }

    fn render(&mut self, status: CoordinatorStatus) {
        let (text, icon) = match (&status.recording_path, &status.meeting) {
            (Some(path), _) => (format!("Recording: {}", file_name(path)), &self.recording_icon),
            (None, Some(rule)) => (format!("In a meeting (not recording): {rule}"), &self.meeting_icon),
            (None, None) => ("Idle".to_owned(), &self.idle_icon),
        };
        self.status_item.set_text(&text);
        self.record_item.set_text(if status.is_recording() {
            "Stop recording"
        } else {
            "Start recording now"
        });
        self.auto_item.set_checked(status.auto_enabled);
        let _ = self.icon.set_icon(Some(icon.clone()));
        let _ = self.icon.set_tooltip(Some(format!("{APP_NAME} - {text}")));
        self.status = Some(status);
    }
}

fn show_notice(notice: &Notice) {
    let title = match notice.level {
        NoticeLevel::Info => APP_NAME.to_owned(),
        NoticeLevel::Warning => format!("{APP_NAME} - Warning"),
        NoticeLevel::Error => format!("{APP_NAME} - Error"),
    };
    platform::notify(&title, &notice.message);
}

fn open_directory(path: &Path) {
    if let Err(error) = std::fs::create_dir_all(path) {
        log::warn!("cannot create folder ({}): {error}", path.display());
        return;
    }
    open(path);
}

fn open(path: &Path) {
    if let Err(error) = open::that_detached(path) {
        log::warn!("cannot open ({}): {error}", path.display());
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Creates an icon of a circle in the given color.
fn circle_icon(color: [u8; 3]) -> anyhow::Result<Icon> {
    const SIZE: u32 = 32;
    let center = (SIZE as f32 - 1.0) / 2.0;
    let radius = SIZE as f32 / 2.0 - 3.0;

    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let distance = ((x as f32 - center).powi(2) + (y as f32 - center).powi(2)).sqrt();
            // Smooth the edge over a width of 1 pixel.
            let alpha = (radius + 0.5 - distance).clamp(0.0, 1.0);
            rgba.extend_from_slice(&[color[0], color[1], color[2], (alpha * 255.0) as u8]);
        }
    }
    Ok(Icon::from_rgba(rgba, SIZE, SIZE)?)
}
