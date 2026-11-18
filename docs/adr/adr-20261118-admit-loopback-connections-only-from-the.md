---
id: adr-20261118-admit-loopback-connections-only-from-the
kind: adr
title: Admit loopback connections only from the current user's processes
status: proposed
created: '2026-11-18'
decision_makers:
- assistant
tags: []
owners: []
relations:
- {type: references, target: design-browser-tab-recording}
source_paths:
- crates/app/src/browser/peer.rs
- crates/app/src/browser/server.rs
- crates/app/src/platform/mac/peers.rs
- crates/app/src/platform/win/peers.rs
summary: Both loopback listeners ask the operating system which user owns the other
  end of a new connection and close it unread unless it is the user the app runs as;
  tokens are checked after that.
---

# Admit loopback connections only from the current user's processes

## Context

The app listens on two kinds of loopback port: the control listener for the relay, and a listener per tab recording for the extension's upload WebSocket. Any process on the computer, another user's included, can connect to a loopback port. A connection proves itself with a token only after the app has read from it, so the app has to hold connections that have not proved themselves yet, and it can hold only a few. An automated security review on 2026-11-18 found that a slow peer could keep the extension's upload out until the recording failed to start. Advancing waiting connections side by side with a fixed time each, and dropping the oldest when too many wait, stopped one slow peer, but a process that kept opening connections could still push the extension's connection out before it was read. The user asked for a structural fix rather than a local one.

## Decision

When a listener accepts a connection, it asks the operating system which user owns the other end, through the `LoopbackPeers` port, before reading anything. A connection is admitted only when that user is the one the app runs as; another user's connection, and one whose owner cannot be found, is closed unread. The tokens, the size and time limits and the cap on waiting connections stay as they are and apply to admitted connections.

- macOS reads the kernel's list of TCP connections (`net.inet.tcp.pcblist64`), which names the user of every socket. No other process is opened, and one call covers every connection.
- Windows finds the owning process in the TCP table (`GetExtendedTcpTable`) and reads the user from that process's token.
- Whether to admit is a pure function of the owner and the current user (`browser::peer::admit`); the platform code only reports what the system says.

## Alternatives considered

- **More places, shorter times.** Any fixed number of places can be filled by a process that opens connections faster than the app reads them, and the extension's connection is indistinguishable from the others until it is read.
- **The token in the upload URL, checked on the first bytes.** It shortens the window in which the extension's connection can be pushed out but does not close it: a connection can be accepted before its first bytes arrive.
- **Only the linked browser's process tree.** It would also keep out the current user's other processes, but it needs the process tree and the browser's network process on each platform, and the current user's processes are already inside the trust boundary: they can read the token file.

## Consequences

- Positive: another user's processes cannot read, slow down or crowd out either listener; their connections are closed before anything is read.
- Positive: the trust boundary the token file already relied on, the current user, is enforced where connections arrive, in the same way on both listeners.
- Negative: every accepted connection costs a system lookup. On macOS the list holds every TCP connection on the computer; on one Mac on 2026-11-18, with 106 connections, reading and searching it took about 0.1 ms.
- Negative: the current user's own processes can still crowd out the extension's connection. They are trusted, as they can read the token and stop the app anyway.
- Neutral: the Windows lookup has been type-checked only. If it fails there, the browser link and tab recording stop working and the log names the reason. Whether a sandboxed browser network process lets the app read its token was not established.
