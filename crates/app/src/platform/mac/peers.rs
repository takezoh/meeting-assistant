//! Who owns the other end of a loopback connection on macOS, read from the kernel's list of TCP connections
//! (`net.inet.tcp.pcblist64`). The list names the user of every socket, so no other process has to be opened, and
//! one call answers for all connections.

use std::ffi::CStr;
use std::ptr;

use crate::browser::peer::{LoopbackPeers, UserId};

const PCB_LIST: &CStr = c"net.inet.tcp.pcblist64";

/// Where the fields this module reads lie in a `struct xtcpcb64` record, measured with `offsetof` in the macOS 26 SDK
/// (`<netinet/tcp_var.h>`, `<netinet/in_pcb.h>`, `<sys/socketvar.h>`; the structures are packed to 4 bytes). Ports
/// and addresses are in network byte order, the uid in the machine's.
mod layout {
    /// Size of `struct xinpgen`, which starts and ends the list. A record no longer than this is the end.
    pub const XINPGEN: usize = 24;
    pub const FOREIGN_PORT: usize = 20;
    pub const LOCAL_PORT: usize = 22;
    /// `inp_vflag`; bit `INP_IPV4` marks an IPv4 connection.
    pub const VERSION_FLAGS: usize = 96;
    pub const INP_IPV4: u8 = 0x1;
    pub const FOREIGN_ADDRESS: usize = 112;
    pub const LOCAL_ADDRESS: usize = 128;
    /// `xi_socket.so_uid`.
    pub const UID: usize = 252;
}

const LOOPBACK: [u8; 4] = [127, 0, 0, 1];

pub struct MacLoopbackPeers;

impl LoopbackPeers for MacLoopbackPeers {
    fn owner(&self, server_port: u16, client_port: u16) -> anyhow::Result<Option<UserId>> {
        let list = tcp_connections()?;
        Ok(owner_in(&list, client_port, server_port).map(|uid| UserId(uid.to_string())))
    }

    fn current_user(&self) -> anyhow::Result<UserId> {
        // SAFETY: geteuid cannot fail.
        Ok(UserId(unsafe { libc::geteuid() }.to_string()))
    }
}

/// The kernel's list of TCP connections, as it hands it over.
fn tcp_connections() -> anyhow::Result<Vec<u8>> {
    // The list can grow between asking for its size and reading it; then the read fails with ENOMEM.
    for _ in 0..3 {
        let mut size = 0usize;
        // SAFETY: a size query with no output buffer.
        let status = unsafe { libc::sysctlbyname(PCB_LIST.as_ptr(), ptr::null_mut(), &mut size, ptr::null_mut(), 0) };
        anyhow::ensure!(
            status == 0,
            "cannot read the list of TCP connections: {}",
            std::io::Error::last_os_error()
        );
        size += size / 4 + 4096;
        let mut list = vec![0u8; size];
        // SAFETY: the buffer holds `size` bytes, and the kernel writes at most that many and updates `size`.
        let status = unsafe {
            libc::sysctlbyname(
                PCB_LIST.as_ptr(),
                list.as_mut_ptr().cast(),
                &mut size,
                ptr::null_mut(),
                0,
            )
        };
        if status == 0 {
            list.truncate(size);
            return Ok(list);
        }
        let error = std::io::Error::last_os_error();
        anyhow::ensure!(
            error.raw_os_error() == Some(libc::ENOMEM),
            "cannot read the list of TCP connections: {error}"
        );
    }
    anyhow::bail!("the list of TCP connections kept growing while it was read")
}

/// The uid that owns the socket at 127.0.0.1:`local_port` connected to 127.0.0.1:`foreign_port`, from a list as
/// [`tcp_connections`] returns it.
fn owner_in(list: &[u8], local_port: u16, foreign_port: u16) -> Option<u32> {
    let mut offset = read_u32(list, 0)? as usize;
    loop {
        let length = read_u32(list, offset)? as usize;
        if length <= layout::XINPGEN {
            return None;
        }
        let record = list.get(offset..offset + length)?;
        let is_match = record.get(layout::VERSION_FLAGS)? & layout::INP_IPV4 != 0
            && read_port(record, layout::LOCAL_PORT)? == local_port
            && read_port(record, layout::FOREIGN_PORT)? == foreign_port
            && record.get(layout::LOCAL_ADDRESS..layout::LOCAL_ADDRESS + 4)? == LOOPBACK
            && record.get(layout::FOREIGN_ADDRESS..layout::FOREIGN_ADDRESS + 4)? == LOOPBACK;
        if is_match {
            return read_u32(record, layout::UID);
        }
        offset += length;
    }
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_ne_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn read_port(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, TcpListener, TcpStream};

    use super::*;

    const RECORD: usize = 472;

    fn header() -> Vec<u8> {
        let mut bytes = vec![0u8; layout::XINPGEN];
        bytes[..4].copy_from_slice(&(layout::XINPGEN as u32).to_ne_bytes());
        bytes
    }

    fn record(local: ([u8; 4], u16), foreign: ([u8; 4], u16), uid: u32) -> Vec<u8> {
        let mut bytes = vec![0u8; RECORD];
        bytes[..4].copy_from_slice(&(RECORD as u32).to_ne_bytes());
        bytes[layout::VERSION_FLAGS] = layout::INP_IPV4;
        bytes[layout::LOCAL_PORT..][..2].copy_from_slice(&local.1.to_be_bytes());
        bytes[layout::FOREIGN_PORT..][..2].copy_from_slice(&foreign.1.to_be_bytes());
        bytes[layout::LOCAL_ADDRESS..][..4].copy_from_slice(&local.0);
        bytes[layout::FOREIGN_ADDRESS..][..4].copy_from_slice(&foreign.0);
        bytes[layout::UID..][..4].copy_from_slice(&uid.to_ne_bytes());
        bytes
    }

    #[test]
    fn the_owner_is_read_from_the_record_of_the_client_end() {
        let list = [
            header(),
            // The server's own end of the same connection, and a connection elsewhere with the same ports.
            record((LOOPBACK, 4000), (LOOPBACK, 51000), 501),
            record(([10, 0, 0, 2], 51000), ([10, 0, 0, 1], 4000), 0),
            record((LOOPBACK, 51000), (LOOPBACK, 4000), 502),
            header(),
        ]
        .concat();

        assert_eq!(owner_in(&list, 51000, 4000), Some(502));
        assert_eq!(owner_in(&list, 51001, 4000), None);
    }

    #[test]
    fn a_short_or_cut_list_finds_nothing() {
        assert_eq!(owner_in(&[], 1, 2), None);
        let mut list = [header(), record((LOOPBACK, 1), (LOOPBACK, 2), 501)].concat();
        list.truncate(list.len() - 10);
        assert_eq!(owner_in(&list, 1, 2), None);
    }

    /// Reads the real list: this layout must match the running kernel.
    #[test]
    fn this_process_owns_its_own_loopback_connection() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        let (server_port, client_port) = (server.local_addr().unwrap().port(), server.peer_addr().unwrap().port());

        let peers = MacLoopbackPeers;
        assert_eq!(
            peers.owner(server_port, client_port).unwrap(),
            Some(peers.current_user().unwrap())
        );
        drop((client, server));
    }
}
