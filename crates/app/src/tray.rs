//! The notification-area icon, its menu and the status panel that opens from it.
//!
//! Icon: a ring = idle, amber = a meeting is detected but not recorded, red = recording. A left click opens the
//! panel; the menu (a right click) keeps the same actions for when the panel is not available. On macOS the menu
//! bar also shows how long the recording has been running.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::Context;
use meeting_assistant_core::{Config, CoordinatorStatus, Notice, NoticeLevel};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::composition::spawn_engine;
use crate::install::{self, InstallSystem};
use crate::panel::Panel;
use crate::panel::view::{self, Activity, PanelMessage, RecordCommand, Rect};
use crate::platform::{self, PlatformInstaller};
use crate::worker::{Command, Update};

const APP_NAME: &str = "Meeting Assistant";
/// Maximum time to wait for the recording to be finalized on exit.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(60);
/// Whether the menu bar shows the recording's running time next to the icon (only macOS can show text there).
const SHOWS_ELAPSED_TIME: bool = cfg!(target_os = "macos");

enum AppEvent {
    Menu(MenuId),
    Tray(TrayIconEvent),
    Panel(String),
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
    panel_item: MenuItem,
    record_item: MenuItem,
    auto_item: CheckMenuItem,
    open_output: MenuItem,
    open_config: MenuItem,
    open_logs: MenuItem,
    autostart_item: CheckMenuItem,
    /// Shown only when launched from somewhere other than the install location.
    install_item: Option<MenuItem>,
    quit: MenuItem,
    icons: Icons,
    shown: Option<Activity>,
    /// What the record item does now.
    record_command: Option<RecordCommand>,
}

struct Icons {
    idle: Icon,
    meeting: Icon,
    recording: Icon,
}

/// What the event loop keeps between events.
#[derive(Default)]
struct State {
    status: Option<CoordinatorStatus>,
    /// The file being recorded and when this process first saw it recording.
    recording_since: Option<(PathBuf, Instant)>,
    /// When the running time next to the icon changes next.
    next_tick: Option<Instant>,
}

impl State {
    fn elapsed(&self) -> Duration {
        self.recording_since
            .as_ref()
            .map_or(Duration::ZERO, |(_, since)| since.elapsed())
    }

    fn set_status(&mut self, status: CoordinatorStatus) {
        let path = status.recording.as_ref().map(|recording| recording.path.clone());
        self.recording_since = match (path, self.recording_since.take()) {
            (Some(path), Some((seen, since))) if seen == path => Some((seen, since)),
            (Some(path), _) => Some((path, Instant::now())),
            (None, _) => None,
        };
        self.next_tick = self
            .recording_since
            .as_ref()
            .filter(|_| SHOWS_ELAPSED_TIME)
            .map(|_| Instant::now());
        self.status = Some(status);
    }
}

/// Stays resident. Selecting Quit ends the process, so this returns only if setup fails.
pub fn run(config: Config, locations: Locations) -> anyhow::Result<std::convert::Infallible> {
    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();

    let menu_proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let _ = menu_proxy.send_event(AppEvent::Menu(event.id));
    }));
    let tray_proxy = event_loop.create_proxy();
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        let _ = tray_proxy.send_event(AppEvent::Tray(event));
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

    let panel_proxy = event_loop.create_proxy();
    let mut tray: Option<Tray> = None;
    let mut panel: Option<Panel> = None;
    let mut state = State::default();
    // Several exit paths can reach the end of the loop; the recording is finalized by the first one.
    let mut finalized = false;
    event_loop.run(move |event, target, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            // Create the icon after the event loop has started (required on macOS).
            Event::NewEvents(StartCause::Init) => {
                match Tray::create() {
                    Ok(created) => tray = Some(created),
                    Err(error) => {
                        log::error!("cannot create the notification-area icon: {error:#}");
                        *control_flow = ControlFlow::ExitWithCode(1);
                        return;
                    }
                }
                let proxy = panel_proxy.clone();
                match Panel::create(target, move |message| {
                    let _ = proxy.send_event(AppEvent::Panel(message));
                }) {
                    Ok(created) => panel = Some(created),
                    // The menu offers every action too; let a left click open it instead.
                    Err(error) => {
                        log::warn!("cannot create the status panel; the menu remains: {error:#}");
                        if let Some(tray) = &tray {
                            tray.icon.set_show_menu_on_left_click(true);
                            tray.panel_item.set_enabled(false);
                        }
                    }
                }
            }
            Event::UserEvent(AppEvent::Worker(Update::Status(status))) => {
                state.set_status(status);
                if let Some(tray) = &mut tray {
                    render_tray(tray, &state);
                }
                if let Some(panel) = &mut panel {
                    render_panel(panel, &state);
                }
            }
            Event::UserEvent(AppEvent::Worker(Update::Notice(notice))) => show_notice(&notice),
            Event::UserEvent(AppEvent::Tray(TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            })) => {
                if let Some(panel) = &mut panel {
                    render_panel(panel, &state);
                    panel.toggle(Some(Rect {
                        x: rect.position.x,
                        y: rect.position.y,
                        width: f64::from(rect.size.width),
                        height: f64::from(rect.size.height),
                    }));
                }
            }
            Event::UserEvent(AppEvent::Panel(text)) => {
                let Some(message) = view::parse_message(&text) else {
                    log::warn!("ignored an unknown message from the status panel");
                    return;
                };
                match message {
                    PanelMessage::Ready => {
                        if let Some(panel) = &mut panel {
                            panel.page_ready();
                            render_panel(panel, &state);
                        }
                    }
                    PanelMessage::Resize { height } => {
                        if let Some(panel) = &mut panel {
                            panel.content_resized(height);
                        }
                    }
                    PanelMessage::Close => hide(&mut panel),
                    PanelMessage::OpenRecordings => {
                        hide(&mut panel);
                        open_directory(&locations.output_directory);
                    }
                    PanelMessage::OpenConfig => {
                        hide(&mut panel);
                        open(&locations.config_file);
                    }
                    PanelMessage::OpenLogs => {
                        hide(&mut panel);
                        open_directory(&locations.log_directory);
                    }
                    PanelMessage::Quit => {
                        hide(&mut panel);
                        quit(tray.as_ref(), &commands, &mut finalized, control_flow);
                    }
                    other => {
                        if let Some(command) = state
                            .status
                            .as_ref()
                            .and_then(|status| view::command_for(&other, status))
                        {
                            let _ = commands.send(command);
                        }
                    }
                }
            }
            Event::UserEvent(AppEvent::Menu(id)) => {
                let Some(tray) = &mut tray else { return };
                if id == *tray.panel_item.id() {
                    if let Some(panel) = &mut panel {
                        render_panel(panel, &state);
                        panel.show(tray.icon.rect().map(|rect| Rect {
                            x: rect.position.x,
                            y: rect.position.y,
                            width: f64::from(rect.size.width),
                            height: f64::from(rect.size.height),
                        }));
                    }
                } else if id == *tray.record_item.id() {
                    if let Some(command) = tray.record_command.clone() {
                        let _ = commands.send(command.into_command());
                    }
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
                    quit(Some(&*tray), &commands, &mut finalized, control_flow);
                }
            }
            // The panel behaves like a menu: it goes away when something else is clicked.
            Event::WindowEvent {
                window_id,
                event: WindowEvent::Focused(false),
                ..
            } if panel.as_ref().is_some_and(|panel| panel.id() == window_id) => hide(&mut panel),
            // The last event before the process ends. Besides a quit from the menu, it is how the end of the
            // session reaches a macOS app: logout, restart, shutdown and a quit request from another app arrive
            // as a request to terminate the application, and the process exits right after this event.
            Event::LoopDestroyed if !std::mem::replace(&mut finalized, true) => {
                log::info!("The application is being terminated.");
                shut_down(&commands);
            }
            _ => {}
        }

        // Keep the running time next to the icon current while recording.
        if let Some(next) = state.next_tick
            && *control_flow == ControlFlow::Wait
        {
            let now = Instant::now();
            if now >= next {
                if let Some(tray) = &tray {
                    tray.icon.set_title(Some(view::elapsed_label(state.elapsed())));
                }
                let elapsed = state.elapsed();
                let to_next_second = Duration::from_secs(1) - Duration::from_nanos(u64::from(elapsed.subsec_nanos()));
                state.next_tick = Some(now + to_next_second);
            }
            *control_flow = ControlFlow::WaitUntil(state.next_tick.unwrap_or(now));
        }
    })
}

/// Shows the latest status in the tray icon and its menu.
fn render_tray(tray: &mut Tray, state: &State) {
    if let Some(status) = &state.status {
        tray.render(status);
    }
}

/// Shows the latest status in the panel, with the recording's running time as of now.
fn render_panel(panel: &mut Panel, state: &State) {
    if let Some(status) = &state.status {
        panel.render(&view::panel_view(status, state.elapsed(), platform::SCREEN_CAPTURE));
    }
}

fn hide(panel: &mut Option<Panel>) {
    if let Some(panel) = panel {
        panel.hide();
    }
}

fn quit(tray: Option<&Tray>, commands: &mpsc::Sender<Command>, finalized: &mut bool, control_flow: &mut ControlFlow) {
    if let Some(tray) = tray {
        let _ = tray.icon.set_tooltip(Some(format!("{APP_NAME} - Quitting")));
    }
    *finalized = true;
    shut_down(commands);
    *control_flow = ControlFlow::Exit;
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
        let status_item = MenuItem::new("Starting…", false, None);
        let panel_item = MenuItem::new("Show status…", true, None);
        let record_item = MenuItem::new("Start recording", false, None);
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
            &panel_item,
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

        let icons = Icons {
            idle: ring_icon(IDLE_COLOR)?,
            meeting: dot_icon([232, 160, 0])?,
            recording: dot_icon([224, 45, 38])?,
        };
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            // A left click opens the panel; the menu opens with a right click.
            .with_menu_on_left_click(false)
            .with_tooltip(format!("{APP_NAME} - Starting"))
            .build()?;

        let mut tray = Self {
            icon,
            status_item,
            panel_item,
            record_item,
            auto_item,
            open_output,
            open_config,
            open_logs,
            autostart_item,
            install_item,
            quit,
            icons,
            shown: None,
            record_command: None,
        };
        tray.show_icon(Activity::Idle);
        Ok(tray)
    }

    fn render(&mut self, status: &CoordinatorStatus) {
        let summary = view::tray_summary(status, platform::SCREEN_CAPTURE);
        self.status_item.set_text(&summary.status_line);
        self.record_item.set_text(&summary.record_label);
        self.record_item.set_enabled(summary.record_command.is_some());
        self.record_command = summary.record_command;
        self.auto_item.set_checked(status.auto_enabled);
        self.show_icon(summary.activity);
        let _ = self.icon.set_tooltip(Some(summary.tooltip));
        if !status.is_recording() {
            self.icon.set_title(None::<&str>);
        }
    }

    fn show_icon(&mut self, activity: Activity) {
        if self.shown == Some(activity) {
            return;
        }
        let icon = match activity {
            Activity::Idle => &self.icons.idle,
            Activity::Meeting => &self.icons.meeting,
            Activity::Recording => &self.icons.recording,
        };
        // On macOS the idle ring follows the menu bar's color, like the system's own icons.
        #[cfg(target_os = "macos")]
        let shown = if activity == Activity::Idle {
            self.icon.set_icon_templated(Some(icon.clone()))
        } else {
            self.icon.set_icon(Some(icon.clone()))
        };
        #[cfg(not(target_os = "macos"))]
        let shown = self.icon.set_icon(Some(icon.clone()));
        match shown {
            Ok(()) => self.shown = Some(activity),
            Err(error) => log::warn!("cannot change the notification-area icon: {error}"),
        }
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

/// The idle ring. On macOS only its shape counts (it is drawn as a template); elsewhere it is grey, which reads on
/// both light and dark taskbars.
const IDLE_COLOR: [u8; 3] = if cfg!(target_os = "macos") {
    [0, 0, 0]
} else {
    [140, 140, 140]
};
const ICON_SIZE: u32 = 32;

/// A filled circle in the given color.
fn dot_icon(color: [u8; 3]) -> anyhow::Result<Icon> {
    circle_icon(color, |distance, radius| radius + 0.5 - distance)
}

/// An outlined circle in the given color.
fn ring_icon(color: [u8; 3]) -> anyhow::Result<Icon> {
    const STROKE: f32 = 3.0;
    circle_icon(color, |distance, radius| {
        let outer = radius + 0.5 - distance;
        let inner = distance - (radius - STROKE) + 0.5;
        outer.min(inner)
    })
}

/// Draws a circle centered in the icon. `coverage(distance, radius)` gives each pixel's opacity from its distance
/// to the center; values are clamped to 0..=1, which smooths the edges over a width of one pixel.
fn circle_icon(color: [u8; 3], coverage: impl Fn(f32, f32) -> f32) -> anyhow::Result<Icon> {
    let center = (ICON_SIZE as f32 - 1.0) / 2.0;
    let radius = ICON_SIZE as f32 / 2.0 - 4.0;

    let mut rgba = Vec::with_capacity((ICON_SIZE * ICON_SIZE * 4) as usize);
    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let distance = ((x as f32 - center).powi(2) + (y as f32 - center).powi(2)).sqrt();
            let alpha = coverage(distance, radius).clamp(0.0, 1.0);
            rgba.extend_from_slice(&[color[0], color[1], color[2], (alpha * 255.0) as u8]);
        }
    }
    Ok(Icon::from_rgba(rgba, ICON_SIZE, ICON_SIZE)?)
}
