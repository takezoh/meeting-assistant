//! Behavior as the relay process launched by the browser. Carries data between the extension (stdio) and the app (TCP).

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Shutdown, TcpStream};

use anyhow::Context;

use super::endpoint::Endpoint;
use super::protocol::{hello_body, write_frame};

/// Copies everything read from one side to the other until either side is closed.
/// If `flush_each` is true, flushes after every write (so each message is delivered immediately).
fn pump(mut from: impl Read, mut to: impl Write, flush_each: bool) {
    let mut buffer = vec![0u8; 256 * 1024];
    while let Ok(length @ 1..) = from.read(&mut buffer) {
        let written = to
            .write_all(&buffer[..length])
            .and_then(|()| if flush_each { to.flush() } else { Ok(()) });
        if written.is_err() {
            break;
        }
    }
}

/// Relays until the browser closes the connection or the connection to the app is lost.
pub fn run() -> anyhow::Result<()> {
    let endpoint = Endpoint::discover()?;
    let mut to_app = TcpStream::connect((Ipv4Addr::LOCALHOST, endpoint.port)).context("Cannot connect to the app")?;
    write_frame(&mut to_app, &hello_body(&endpoint.token, std::process::id()))?;

    let from_app = to_app.try_clone()?;
    std::thread::spawn(move || {
        // App -> extension. When the connection to the app drops, this process exits too.
        pump(from_app, io::stdout().lock(), true);
        std::process::exit(0);
    });

    // Extension -> app.
    pump(io::stdin().lock(), &to_app, false);
    let _ = to_app.shutdown(Shutdown::Both);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::pump;

    #[test]
    fn copies_what_it_reads_unchanged() {
        let source: Vec<u8> = (0..600_000u32).map(|i| (i % 251) as u8).collect();
        let mut written = Vec::new();

        pump(Cursor::new(source.clone()), &mut written, true);

        assert_eq!(written, source);
    }
}
