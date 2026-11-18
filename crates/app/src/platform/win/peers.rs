//! Who owns the other end of a loopback connection on Windows: the owning process from the system's TCP table
//! (`GetExtendedTcpTable`), then the user of that process's token as a SID string.

use windows::Win32::Foundation::{CloseHandle, ERROR_INSUFFICIENT_BUFFER, HANDLE, HLOCAL, LocalFree, NO_ERROR};
use windows::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_CONNECTIONS,
};
use windows::Win32::Networking::WinSock::AF_INET;
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::core::PWSTR;

use crate::browser::peer::{LoopbackPeers, UserId};

const LOOPBACK: [u8; 4] = [127, 0, 0, 1];

pub struct WindowsLoopbackPeers;

impl LoopbackPeers for WindowsLoopbackPeers {
    fn owner(&self, server_port: u16, client_port: u16) -> anyhow::Result<Option<UserId>> {
        let connections = tcp_connections()?;
        let Some(process_id) = owner_in(&connections, client_port, server_port) else {
            return Ok(None);
        };
        // SAFETY: the handle is closed by the guard.
        let process = Handle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }?);
        user_of(&process).map(Some)
    }

    fn current_user(&self) -> anyhow::Result<UserId> {
        // SAFETY: the pseudo handle of this process needs no closing.
        user_of(&unsafe { GetCurrentProcess() })
    }
}

/// Closes a handle when dropped.
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by this module and is closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

impl std::ops::Deref for Handle {
    type Target = HANDLE;

    fn deref(&self) -> &HANDLE {
        &self.0
    }
}

/// The system's IPv4 TCP connections with their owning processes.
fn tcp_connections() -> anyhow::Result<Vec<MIB_TCPROW_OWNER_PID>> {
    let mut size = 0u32;
    for _ in 0..3 {
        // Rows are u32 fields; keep the buffer aligned for them, with room for connections opened meanwhile.
        let mut buffer = vec![0u32; (size as usize).div_ceil(4) + 256];
        let mut length = (buffer.len() * 4) as u32;
        // SAFETY: the buffer holds `length` bytes.
        let status = unsafe {
            GetExtendedTcpTable(
                Some(buffer.as_mut_ptr().cast()),
                &mut length,
                false,
                u32::from(AF_INET.0),
                TCP_TABLE_OWNER_PID_CONNECTIONS,
                0,
            )
        };
        if status == ERROR_INSUFFICIENT_BUFFER.0 {
            size = length;
            continue;
        }
        anyhow::ensure!(
            status == NO_ERROR.0,
            "cannot read the TCP connection table (error {status})"
        );
        // SAFETY: the table starts with its row count, followed by that many rows, all within `length` bytes.
        unsafe {
            let table = &*buffer.as_ptr().cast::<MIB_TCPTABLE_OWNER_PID>();
            let count = table.dwNumEntries as usize;
            let room = (length as usize).saturating_sub(4) / size_of::<MIB_TCPROW_OWNER_PID>();
            anyhow::ensure!(count <= room, "the TCP connection table is cut short");
            return Ok(std::slice::from_raw_parts(table.table.as_ptr(), count).to_vec());
        }
    }
    anyhow::bail!("the TCP connection table kept growing while it was read")
}

/// The process that owns the connection at 127.0.0.1:`local_port` to 127.0.0.1:`remote_port`. The table keeps ports
/// in network byte order in the low 16 bits and addresses in network byte order.
fn owner_in(rows: &[MIB_TCPROW_OWNER_PID], local_port: u16, remote_port: u16) -> Option<u32> {
    let port = |value: u32| u16::from_be(value as u16);
    let address = |value: u32| value.to_ne_bytes();
    rows.iter()
        .find(|row| {
            port(row.dwLocalPort) == local_port
                && port(row.dwRemotePort) == remote_port
                && address(row.dwLocalAddr) == LOOPBACK
                && address(row.dwRemoteAddr) == LOOPBACK
        })
        .map(|row| row.dwOwningPid)
}

/// The user a process runs as, as a SID string.
fn user_of(process: &HANDLE) -> anyhow::Result<UserId> {
    let mut token = HANDLE::default();
    // SAFETY: the token handle is closed by the guard.
    unsafe { OpenProcessToken(*process, TOKEN_QUERY, &mut token) }?;
    let token = Handle(token);

    let mut needed = 0u32;
    // SAFETY: a size query; it fails with the size needed.
    let _ = unsafe { GetTokenInformation(*token, TokenUser, None, 0, &mut needed) };
    let mut buffer = vec![0u64; (needed as usize).div_ceil(8)];
    // SAFETY: the buffer holds `needed` bytes, aligned for TOKEN_USER.
    unsafe { GetTokenInformation(*token, TokenUser, Some(buffer.as_mut_ptr().cast()), needed, &mut needed) }?;
    // SAFETY: GetTokenInformation wrote a TOKEN_USER at the start of the buffer, whose SID lies in the buffer.
    let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };

    let mut text = PWSTR::null();
    // SAFETY: the SID is valid while the buffer lives; the string is freed with LocalFree.
    unsafe {
        ConvertSidToStringSidW(sid, &mut text)?;
        let user = text.to_string();
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
        Ok(UserId(user?))
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, TcpListener, TcpStream};

    use super::*;

    /// A row as the system fills it in.
    fn row(local: ([u8; 4], u16), remote: ([u8; 4], u16), process_id: u32) -> MIB_TCPROW_OWNER_PID {
        MIB_TCPROW_OWNER_PID {
            dwState: 5,
            dwLocalAddr: u32::from_ne_bytes(local.0),
            dwLocalPort: u32::from(local.1.to_be()),
            dwRemoteAddr: u32::from_ne_bytes(remote.0),
            dwRemotePort: u32::from(remote.1.to_be()),
            dwOwningPid: process_id,
        }
    }

    #[test]
    fn the_owner_is_the_process_of_the_client_end() {
        let rows = [
            row((LOOPBACK, 4000), (LOOPBACK, 51000), 10),
            row(([10, 0, 0, 2], 51000), ([10, 0, 0, 1], 4000), 11),
            row((LOOPBACK, 51000), (LOOPBACK, 4000), 12),
        ];

        assert_eq!(owner_in(&rows, 51000, 4000), Some(12));
        assert_eq!(owner_in(&rows, 51001, 4000), None);
    }

    /// Reads the real table and token: runs on Windows only.
    #[test]
    fn this_process_owns_its_own_loopback_connection() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        let (server_port, client_port) = (server.local_addr().unwrap().port(), server.peer_addr().unwrap().port());

        let peers = WindowsLoopbackPeers;
        assert_eq!(
            peers.owner(server_port, client_port).unwrap(),
            Some(peers.current_user().unwrap())
        );
        drop((client, server));
    }
}
