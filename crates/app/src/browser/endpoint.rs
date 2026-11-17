//! Where the app is listening for connections from the extension.
//! The app writes it; the relay process launched by the browser reads it.

use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    pub port: u16,
    /// Shared secret that keeps unrelated processes on the same PC from connecting.
    pub token: String,
}

fn file() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_default()
        .join("MeetingAssistant")
        .join("browser-link.json")
}

impl Endpoint {
    /// Writes the endpoint to a place the relay process can read.
    pub fn publish(&self) -> anyhow::Result<()> {
        let path = file();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, serde_json::to_vec(self)?)
            .with_context(|| format!("Cannot write the endpoint: {}", path.display()))
    }

    /// Reads the endpoint the app wrote. Fails if the app has never been started.
    pub fn discover() -> anyhow::Result<Self> {
        let body = std::fs::read(file()).context("The app is not running")?;
        serde_json::from_slice(&body).context("The endpoint file is corrupted")
    }
}
