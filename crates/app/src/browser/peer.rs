//! Who may connect to the app's loopback listeners. Deciding only; what the operating system knows about a
//! connection comes through the [`LoopbackPeers`] port.
//!
//! Any process on the computer can connect to a loopback port, another user's included. The app's trust boundary is
//! the current user: the control token is kept in a file only that user can read. The listeners enforce the same
//! boundary when they accept a connection, before reading anything from it, so that another user's processes cannot
//! occupy the few places kept for connections that have not presented a token yet.

/// A user account as the operating system names it: a uid on macOS, a SID on Windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserId(pub String);

/// What the operating system knows about IPv4 loopback connections. The real one is
/// `platform::PlatformLoopbackPeers`.
pub trait LoopbackPeers: Send + Sync {
    /// The user running the process that owns the client end of the loopback connection from `client_port` to
    /// this app's `server_port`. `None` when the system lists no such connection, for example because it closed.
    fn owner(&self, server_port: u16, client_port: u16) -> anyhow::Result<Option<UserId>>;

    /// The user this app runs as.
    fn current_user(&self) -> anyhow::Result<UserId>;
}

/// Whether a just-accepted connection whose client end belongs to `owner` may be read at all. A connection whose owner
/// is not known is refused too. The reason is for the log.
pub fn admit(owner: anyhow::Result<Option<UserId>>, current: &UserId) -> Result<(), String> {
    match owner {
        Ok(Some(owner)) if &owner == current => Ok(()),
        Ok(Some(owner)) => Err(format!("it comes from another user's process ({})", owner.0)),
        Ok(None) => Err("the system does not list it".to_owned()),
        Err(error) => Err(format!("its owner cannot be looked up: {error:#}")),
    }
}

#[cfg(test)]
pub mod fakes {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    pub const CURRENT_USER: &str = "current";

    /// Every connection belongs to the current user, until told that they come from another user.
    #[derive(Default)]
    pub struct FakeLoopbackPeers {
        foreign: AtomicBool,
    }

    impl FakeLoopbackPeers {
        pub fn set_foreign(&self, foreign: bool) {
            self.foreign.store(foreign, Ordering::SeqCst);
        }
    }

    impl LoopbackPeers for FakeLoopbackPeers {
        fn owner(&self, _server_port: u16, _client_port: u16) -> anyhow::Result<Option<UserId>> {
            Ok(Some(UserId(
                if self.foreign.load(Ordering::SeqCst) {
                    "another"
                } else {
                    CURRENT_USER
                }
                .into(),
            )))
        }

        fn current_user(&self) -> anyhow::Result<UserId> {
            Ok(UserId(CURRENT_USER.into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn me() -> UserId {
        UserId("501".into())
    }

    #[test]
    fn only_a_connection_owned_by_the_current_user_is_admitted() {
        assert_eq!(admit(Ok(Some(me())), &me()), Ok(()));

        let refused = admit(Ok(Some(UserId("502".into()))), &me()).unwrap_err();
        assert!(refused.contains("another user") && refused.contains("502"), "{refused}");
    }

    #[test]
    fn a_connection_whose_owner_is_not_known_is_refused() {
        assert!(admit(Ok(None), &me()).is_err());
        let refused = admit(Err(anyhow::anyhow!("access denied")), &me()).unwrap_err();
        assert!(refused.contains("access denied"), "{refused}");
    }
}
