//! The Tauri shell. Everything the window shows comes from `engine_client`; the shell only forwards
//! frames and commands. Build with `--features tauri-app` (default) for the real window, or with
//! `--no-default-features` to compile and test the engine client headlessly.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod engine_client;

#[cfg(feature = "tauri-app")]
mod shell {
    use crate::engine_client::{EngineClient, ViewState};
    use ma_ipc::Transport;
    use std::sync::Mutex;

    /// The named-pipe transport arrives with the Windows unit; until then the shell has no engine
    /// and renders the disconnected state.
    pub struct NoTransport;
    impl Transport for NoTransport {
        fn send(&mut self, _frame: &ma_ipc::Frame) -> Result<(), ma_ipc::TransportError> {
            Err(ma_ipc::TransportError::Closed)
        }
        fn recv(&mut self) -> Result<Option<ma_ipc::Frame>, ma_ipc::TransportError> {
            Err(ma_ipc::TransportError::Closed)
        }
        fn close(&mut self) {}
        fn is_closed(&self) -> bool {
            true
        }
    }

    pub struct AppState(pub Mutex<EngineClient<NoTransport>>);

    #[tauri::command]
    pub fn view(state: tauri::State<'_, AppState>) -> ViewState {
        state.0.lock().expect("client").view().clone()
    }

    #[tauri::command]
    pub fn cancel_arming(state: tauri::State<'_, AppState>) {
        state.0.lock().expect("client").cancel_arming();
    }

    #[tauri::command]
    pub fn start(state: tauri::State<'_, AppState>) {
        state.0.lock().expect("client").start();
    }

    pub fn run() {
        tauri::Builder::default()
            .manage(AppState(Mutex::new(EngineClient::new())))
            .invoke_handler(tauri::generate_handler![view, cancel_arming, start])
            .run(tauri::generate_context!())
            .expect("tauri shell runs");
    }
}

fn main() {
    #[cfg(feature = "tauri-app")]
    shell::run();
    #[cfg(not(feature = "tauri-app"))]
    eprintln!("app-ui built headless: the engine client compiles and tests without a window");
}
