//! The panel's window: a borderless window next to the tray icon that shows the panel's page in a web view.
//! What it shows and where it goes are decided in `view`; this module only creates, moves and fills the window.
//!
//! The web view and the processes the system's web runtime starts for it hold far more memory than the rest of the
//! app, and the panel is closed nearly all the time. So the web view is let go some time after the panel closes and
//! made again when it next opens; the window itself stays.

use std::rc::Rc;
use std::time::{Duration, Instant};

use tao::dpi::{LogicalSize, PhysicalPosition};
use tao::event_loop::EventLoopWindowTarget;
use tao::window::{Window, WindowBuilder, WindowId};
use wry::{NewWindowResponse, WebView, WebViewBuilder};

use super::view::{self, PanelView, Rect, Surface};
use crate::platform;

/// Width of the panel in logical pixels. The height follows the content.
const WIDTH: f64 = 360.0;
const INITIAL_HEIGHT: f64 = 300.0;
const MIN_HEIGHT: f64 = 160.0;
/// Space kept between the panel and the tray icon, and between the panel and the screen's edges, in logical pixels.
const GAP: f64 = 6.0;
/// A click on the tray icon while the panel is open first takes the focus from the panel, which hides it. A click
/// that arrives this soon after hiding is that same click and must not open the panel again.
const REOPEN_GUARD: Duration = Duration::from_millis(400);
/// How long the web view is kept after the panel closes. Opening again within this time is immediate; after it, the
/// next opening waits for the page to be made, a fraction of a second.
const KEEP_AFTER_CLOSING: Duration = Duration::from_secs(30);

pub struct Panel {
    window: Window,
    surface: Surface,
    on_message: Rc<dyn Fn(String)>,
    /// The page, while it is kept.
    webview: Option<WebView>,
    /// The page has loaded and accepts content.
    ready: bool,
    /// The latest content, as the script that shows it. Kept, so that a page made again shows it as well.
    latest: Option<String>,
    /// The panel was asked to open while its page was being made; it opens once the page is ready.
    opening: bool,
    /// The tray icon's position in physical pixels when the panel was last opened.
    anchor: Option<Rect>,
    content_height: f64,
    hidden_at: Option<Instant>,
    /// When the web view is let go if the panel stays closed.
    release_at: Option<Instant>,
}

impl Panel {
    /// Creates the window, hidden. `on_message` receives each message the page sends.
    pub fn create<T: 'static>(
        target: &EventLoopWindowTarget<T>,
        on_message: impl Fn(String) + 'static,
    ) -> anyhow::Result<Self> {
        let builder = WindowBuilder::new()
            .with_title("Meeting Assistant")
            .with_inner_size(LogicalSize::new(WIDTH, INITIAL_HEIGHT))
            .with_decorations(false)
            .with_resizable(false)
            .with_visible(false)
            .with_focused(true)
            .with_always_on_top(true)
            .with_visible_on_all_workspaces(true);
        let window = platform::panel_window(builder).build(target)?;
        let surface = platform::style_panel(&window);
        let on_message: Rc<dyn Fn(String)> = Rc::new(on_message);
        // Made now, so that a system without the web runtime is noticed at start and the menu takes over.
        let webview = make_page(&window, surface, &on_message)?;

        Ok(Self {
            window,
            surface,
            on_message,
            webview: Some(webview),
            ready: false,
            latest: None,
            opening: false,
            anchor: None,
            content_height: INITIAL_HEIGHT,
            hidden_at: None,
            release_at: Some(Instant::now() + KEEP_AFTER_CLOSING),
        })
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    pub fn is_visible(&self) -> bool {
        self.window.is_visible()
    }

    /// Opens the panel next to `anchor`, or closes it if it is open.
    pub fn toggle(&mut self, anchor: Option<Rect>) {
        if self.is_visible() {
            self.hide();
        } else if self.hidden_at.is_none_or(|at| at.elapsed() > REOPEN_GUARD) {
            self.show(anchor);
        }
    }

    /// Opens the panel. If its page was let go, the page is made again and the panel opens once it is ready.
    pub fn show(&mut self, anchor: Option<Rect>) {
        self.anchor = anchor.or(self.anchor);
        self.release_at = None;
        if self.webview.is_none() {
            match make_page(&self.window, self.surface, &self.on_message) {
                Ok(webview) => {
                    self.webview = Some(webview);
                    self.ready = false;
                }
                Err(error) => {
                    log::warn!("cannot make the panel's page again: {error:#}");
                    return;
                }
            }
        }
        if self.ready {
            self.reveal();
        } else {
            self.opening = true;
        }
    }

    fn reveal(&mut self) {
        let Some(webview) = &self.webview else { return };
        self.opening = false;
        self.place();
        let _ = webview.set_visible(true);
        platform::panel_shown(webview, true);
        self.window.set_visible(true);
        self.window.set_focus();
        let _ = webview.focus();
    }

    /// Hides the window and tells the web view, so that the hidden page stops drawing and its timers slow down.
    pub fn hide(&mut self) {
        self.opening = false;
        if self.is_visible() {
            self.window.set_visible(false);
            if let Some(webview) = &self.webview {
                let _ = webview.set_visible(false);
                platform::panel_shown(webview, false);
            }
            let now = Instant::now();
            self.hidden_at = Some(now);
            self.release_at = Some(now + KEEP_AFTER_CLOSING);
        }
    }

    /// When the panel's page is to be let go, if the panel stays closed until then.
    pub fn release_due(&self) -> Option<Instant> {
        self.webview.as_ref().and(self.release_at)
    }

    /// Lets the page go if the panel has stayed closed long enough. The web runtime's processes end with it.
    pub fn release_if_due(&mut self, now: Instant) {
        if self.release_due().is_some_and(|due| now >= due) && !self.is_visible() && !self.opening {
            self.webview = None;
            self.ready = false;
            self.release_at = None;
        }
    }

    /// Shows new content, now if the page is ready, otherwise as soon as it is.
    pub fn render(&mut self, view: &PanelView) {
        match serde_json::to_string(view) {
            Ok(json) => {
                self.latest = Some(format!("window.render({json})"));
                self.flush();
            }
            Err(error) => log::warn!("cannot prepare the panel's content: {error}"),
        }
    }

    /// The page has loaded. It is given the latest content, and opens if it was asked to meanwhile.
    pub fn page_ready(&mut self) {
        self.ready = true;
        self.flush();
        if self.opening {
            self.reveal();
        }
    }

    /// Fits the window to the page's content.
    pub fn content_resized(&mut self, height: f64) {
        if height.is_finite() && (height - self.content_height).abs() >= 1.0 {
            self.content_height = height.max(MIN_HEIGHT);
            self.place();
        }
    }

    fn flush(&mut self) {
        if !self.ready {
            return;
        }
        if let (Some(webview), Some(script)) = (&self.webview, &self.latest)
            && let Err(error) = webview.evaluate_script(script)
        {
            log::warn!("cannot update the panel: {error}");
        }
    }

    /// Sizes the window to its content and puts it next to the tray icon, on the screen that holds the icon.
    fn place(&self) {
        let point = self
            .anchor
            .map(|a| (a.x + a.width / 2.0, a.y + a.height / 2.0))
            .unwrap_or_default();
        let Some(monitor) = self
            .window
            .monitor_from_point(point.0, point.1)
            .or_else(|| self.window.primary_monitor())
        else {
            return;
        };
        let scale = monitor.scale_factor();
        let screen = Rect {
            x: f64::from(monitor.position().x),
            y: f64::from(monitor.position().y),
            width: f64::from(monitor.size().width),
            height: f64::from(monitor.size().height),
        };
        let height = (self.content_height * scale).min(screen.height - 2.0 * GAP * scale) / scale;
        self.window.set_inner_size(LogicalSize::new(WIDTH, height));

        // Without a known icon position, hang the panel from the top-right corner of the screen.
        let anchor = self.anchor.unwrap_or(Rect {
            x: screen.x + screen.width - (WIDTH / 2.0 + GAP) * scale,
            y: screen.y,
            width: 0.0,
            height: 0.0,
        });
        let (x, y) = view::panel_origin(anchor, (WIDTH * scale, height * scale), screen, GAP * scale);
        self.window.set_outer_position(PhysicalPosition::new(x, y));
    }
}

/// The panel's page in a web view, hidden.
fn make_page(window: &Window, surface: Surface, on_message: &Rc<dyn Fn(String)>) -> anyhow::Result<WebView> {
    let on_message = on_message.clone();
    let webview = WebViewBuilder::new()
        .with_html(view::page(surface))
        .with_transparent(surface.see_through())
        .with_accept_first_mouse(true)
        .with_ipc_handler(move |request| on_message(request.into_body()))
        // The page is self-contained; it never goes anywhere else.
        .with_navigation_handler(|url| url.starts_with("about:") || url.starts_with("data:"))
        .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
        .with_visible(false)
        .build(window)?;
    platform::panel_shown(&webview, false);
    Ok(webview)
}
