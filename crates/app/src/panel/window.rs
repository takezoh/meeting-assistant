//! The panel's window: a borderless window next to the tray icon that shows the panel's page in a web view.
//! What it shows and where it goes are decided in `view`; this module only creates, moves and fills the window.

use std::time::{Duration, Instant};

use tao::dpi::{LogicalSize, PhysicalPosition};
use tao::event_loop::EventLoopWindowTarget;
use tao::window::{Window, WindowBuilder, WindowId};
use wry::{NewWindowResponse, WebView, WebViewBuilder};

use super::view::{self, PanelView, Rect};
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

pub struct Panel {
    window: Window,
    webview: WebView,
    /// The page has loaded and accepts content.
    ready: bool,
    /// The latest content, as the script that shows it.
    pending: Option<String>,
    /// The tray icon's position in physical pixels when the panel was last opened.
    anchor: Option<Rect>,
    content_height: f64,
    hidden_at: Option<Instant>,
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

        let webview = WebViewBuilder::new()
            .with_html(view::page(surface))
            .with_transparent(surface.see_through())
            .with_accept_first_mouse(true)
            .with_ipc_handler(move |request| on_message(request.into_body()))
            // The page is self-contained; it never goes anywhere else.
            .with_navigation_handler(|url| url.starts_with("about:") || url.starts_with("data:"))
            .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
            .build(&window)?;

        Ok(Self {
            window,
            webview,
            ready: false,
            pending: None,
            anchor: None,
            content_height: INITIAL_HEIGHT,
            hidden_at: None,
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

    pub fn show(&mut self, anchor: Option<Rect>) {
        self.anchor = anchor.or(self.anchor);
        self.place();
        self.window.set_visible(true);
        self.window.set_focus();
        let _ = self.webview.focus();
    }

    pub fn hide(&mut self) {
        if self.is_visible() {
            self.window.set_visible(false);
            self.hidden_at = Some(Instant::now());
        }
    }

    /// Shows new content, now if the page is ready, otherwise as soon as it is.
    pub fn render(&mut self, view: &PanelView) {
        match serde_json::to_string(view) {
            Ok(json) => {
                self.pending = Some(format!("window.render({json})"));
                self.flush();
            }
            Err(error) => log::warn!("cannot prepare the panel's content: {error}"),
        }
    }

    /// The page has loaded.
    pub fn page_ready(&mut self) {
        self.ready = true;
        self.flush();
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
        if let Some(script) = self.pending.take()
            && let Err(error) = self.webview.evaluate_script(&script)
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
