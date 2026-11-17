//! Integration with the browser extension.
//!
//! | Module | Responsibility | Side effects |
//! |---|---|---|
//! | [`protocol`] | Message shape and framing | none |
//! | [`hub`] | Connection and recording state transitions | none (output goes through a port) |
//! | [`endpoint`] | Handing over the listening address | file |
//! | [`server`] | Accepting connections, sending commands, writing recording data | sockets, files, threads |
//! | [`relay`] | Relay process launched by the browser | stdin/stdout, sockets |

mod endpoint;
mod hub;
pub mod protocol;
pub mod relay;
mod server;

pub use self::server::BrowserLinkServer;
