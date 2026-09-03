# Meeting Assistant — Development Plan

## 1. Goal

Build a desktop application that automatically detects supported web meetings, records them locally, transcribes and summarizes the conversation, and exports the resulting artifacts to destinations selected by the user.

Supported meeting services:

- Microsoft Teams
- Slack Huddle
- Google Meet
- Zoom

## 2. Top-level requirements

### Provider perspective

- Do not operate a proprietary workflow backend service.
- Minimize failure points caused by changes to external meeting services.

### User perspective

- Do not appear as a bot participant in the meeting.
- Automatically start and stop recording.

These requirements are the primary decision criteria for architecture, scope, and implementation.

## 3. Product principles

### Desktop-first

The product is a desktop application. Desktop and browser-based meetings use the same local detection, capture, and workflow runtime.

### Local workflow ownership

The desktop application owns:

- meeting detection
- recording state
- local artifacts
- workflow state
- retry queues
- transcription and summarization execution
- export state
- retention and deletion

The application's own backend is not part of the workflow data path.

### User-owned integrations

The application connects directly to services configured by the user:

- AI APIs
- local AI runtimes
- Claude Code or Codex CLI
- Google Drive
- Notion
- local folders

Credentials are stored using the operating system's credential store.

### Resilience to meeting-service changes

Meeting detection must not depend on DOM structure, CSS selectors, button labels, screen coordinates, accessibility-tree layout, private APIs, or internal network payloads.

Detection should primarily use stable operating-system signals such as:

- signed application or package identity
- process lifecycle
- audio-session lifecycle
- microphone usage
- audio activity
- calendar context when configured (never a required signal; Microsoft 365 tenants may block third-party Graph access)
- explicit user actions
- for browser meetings only: tab URL host pattern and tab audible state from the detection-only browser extension (public extension APIs, not DOM)

Service-specific behavior must remain isolated behind thin adapters.

## 4. MVP scope

### Platform

- Windows 11

### Meeting applications

- Teams Desktop
- Slack Desktop
- Google Meet in Chrome or Edge, detected by the detection-only browser extension
- Zoom Desktop
- Teams Web and Zoom Web: best effort, same browser detection and capture path as Google Meet

Browser meeting limitations accepted for the MVP:

- meeting audio is captured from the browser process loopback, so audio from other tabs can be mixed into the recording
- without the extension installed, browser meetings fall back to manual start

### Capabilities

- automatic meeting start and end detection
- pre-recording countdown and cancellation
- separate microphone and meeting-application audio capture where supported
- manual start, pause, resume, stop, and discard
- crash-safe recording
- post-meeting transcription
- speaker diarization
- meeting summary
- decisions, action items, and unresolved issues
- local meeting library
- export to local folders, Google Drive, and Notion
- retry and resume after network or application failure

### Non-goals

- meeting participant bots
- browser extensions beyond the detection-only extension (no tab audio capture, no DOM access, no content scripts)
- mobile applications
- Linux support
- real-time translation
- perfect automatic speaker-name identification
- enterprise SSO, DLP, eDiscovery, and centralized administration

## 5. Logical architecture

```text
Meeting applications
        |
        v
Local signal collectors
        |
        v
Meeting detector and session state machine
        |
        v
Local capture engine
        |
        v
Local workflow runtime
   |         |          |
   v         v          v
Processors  Local DB   Destinations
```

Core boundaries:

- **Signal collectors** observe operating-system facts.
- **Meeting detector** decides whether a session is starting or ending.
- **Capture engine** records microphone, application audio, and optional video.
- **Workflow runtime** coordinates processing, retries, recovery, and export.
- **Processors** provide replaceable transcription, diarization, and summarization.
- **Destinations** provide replaceable export implementations.

## 6. Delivery phases

### Phase 0 — Repository and contracts

Deliverables:

- repository structure
- architecture decision records
- meeting-session state model
- signal and detector contracts
- recording and artifact model
- processor contract
- destination contract
- threat model and credential policy
- ADR: the capture engine runs in a process separate from the UI, so UI failure or restart never interrupts recording
- module boundary rules and an automated dependency-direction check that keep meeting-service-specific logic out of the workflow core

Exit criteria:

- core boundaries do not require a proprietary backend
- meeting-service-specific logic cannot leak into the workflow core, enforced by the automated dependency check
- the capture engine can continue recording after the UI process terminates
- workflow steps and artifacts have stable identifiers and states

### Phase 1 — Windows detection and audio-capture PoC

Implement a headless or diagnostic-first prototype for:

- process and package identification
- application audio-session observation
- microphone-use observation
- process-specific loopback capture where available
- manual recording fallback
- signal timeline and detector diagnostics
- microphone endpoint selection that follows the capture session of the meeting application, not the system default device
- measurement of meeting audio leaking into the microphone track when speakers are used
- per-application record of whether process-tree loopback is required
- signal timeline saved in a replayable fixture format, with an optional user confirmation label ("was this a meeting?") per session
- detection-only browser extension PoC: reports tab URL host and audible state to the desktop application over a localhost channel, combined with OS-level browser microphone use for start and end decisions

Test against:

- Teams Desktop
- Slack Huddle
- Google Meet in Chrome, with and without the extension
- Zoom Desktop

Exit criteria:

- microphone and meeting audio can be recorded
- Google Meet start and end are detected from extension signals plus OS microphone use, without DOM access
- two-hour recording completes without losing data
- meeting-start and meeting-end signals are observable for all four targets
- detection requires no UI scraping
- browser audio contamination and other platform limitations are documented
- the recorded microphone matches the device used by the meeting application
- echo conditions and their severity are documented per application
- recorded signal timelines can be replayed as detector test fixtures

### Phase 2 — Local session and workflow runtime

Implement:

- meeting session state machine
- start and end hysteresis
- pre-recording countdown
- local transactional state database
- chunked crash-safe recording
- persistent workflow queue
- idempotent workflow steps
- retry and resume
- meeting and artifact retention

Exit criteria:

- forced application termination can recover the active session
- network loss does not stop local recording
- completed steps are not duplicated after restart
- manual control is always available

### Phase 3 — Transcription, diarization, and summary

Premises:

- the microphone track is the local user and the loopback track is the remote participants; "self vs others" separation comes from the tracks, not from diarization
- supported transcription languages: Japanese and English

Implement replaceable processors for:

- transcription
- diarization
- speaker-label editing
- structured summary generation
- decisions
- action items
- unresolved issues
- evidence links to transcript timestamps

Initial execution options:

- one external API adapter
- one local transcription path

Post-MVP:

- restricted Claude Code / Codex CLI summarization adapter (deferred: subscription CLIs may restrict third-party use, and staged-file isolation needs a dedicated working directory and permission restrictions)

Exit criteria:

- failed chunks can be retried independently
- processor changes do not affect capture or workflow core
- user edits are preserved across regeneration
- every generated decision or action can reference source transcript segments

### Phase 4 — Meeting library and destinations

Implement:

- meeting list and detail views
- synchronized audio and transcript playback
- search
- reprocessing
- deletion and retention controls
- local-folder destination
- Google Drive destination
- Notion database destination
- direct user-owned authentication
  - Google Drive: installed-app OAuth flow with PKCE, `drive.file` scope only
  - Notion: user-created internal integration token, no OAuth redirect
- persistent export retry queue

Exit criteria:

- export is idempotent
- destination outages do not lose local artifacts
- credentials do not appear in application files or logs
- large media can be stored in Drive while Notion stores links and metadata

### Phase 5 — Detection quality and supported-app hardening

Test fixtures are the signal timelines recorded by the Phase 1 diagnostics, labeled with user confirmations collected during ordinary use.

Build a repeatable validation matrix covering:

- application versions
- display languages
- themes and window sizes
- audio-device changes
- Bluetooth reconnects
- long silence
- sleep and resume
- concurrent candidate meetings
- browser meetings with unrelated tab audio
- unknown application versions

Initial targets (desktop applications, and Google Meet with the extension installed):

- start detection success: at least 95%
- end detection success: at least 95%
- false automatic recordings: fewer than one per eight hours of ordinary use

Browser meetings without the extension are excluded from these targets.

Exit criteria:

- detection tests replay recorded signal timelines and do not depend on screenshots or DOM fixtures
- adapter failure falls back safely to generic detection or manual control
- unknown versions use the configured safe fallback
- diagnostics explain the signals used for each decision

### Phase 6 — Video capture and macOS

After the Windows audio-first MVP is stable:

- optional window or display video capture
- macOS signal collectors
- macOS ScreenCaptureKit-based capture
- macOS credential and permission flows
- cross-platform parity review

## 7. Cross-cutting requirements

### Privacy and consent

- recording is always visibly indicated
- users can cancel before automatic recording starts
- users can stop or discard at any time
- external transmission is explicit and configurable
- retention and source-deletion policies are user-controlled
- recording-notice templates are available
- the microphone track is recorded regardless of the meeting application's mute state; this is stated in the UI

### Security

- secrets are stored in the OS credential store
- meeting content and secrets are excluded from diagnostic logs
- CLI processors receive only explicitly staged files
- arbitrary shell commands are not accepted as processor configuration
- all external sends and exports are auditable locally

### Reliability

- local recording continues while offline
- artifacts are chunked and recoverable
- workflow steps are idempotent
- destination failures remain retryable
- processing failure never stops the recording path

## 8. Decisions

### Decided

- the capture engine runs in a process separate from the UI (Phase 0 ADR)
- supported transcription languages: Japanese and English
- Google Drive uses the installed-app OAuth flow with PKCE and the `drive.file` scope
- Notion uses a user-created internal integration token
- the Claude Code / Codex CLI summarization adapter is post-MVP
- Google Meet detection uses a detection-only browser extension (tab URL host and audible state); browser audio stays on the process loopback path, and tab-level audio capture via the extension is post-MVP because it requires a user click per meeting

### Resolve before or during Phase 0

- desktop framework, given the separate capture-engine process
- local database and artifact-directory layout
- audio container, codec, sample rate, and chunk duration
- exact definition and UX of automatic-recording modes
- initial transcription and summarization adapters
- maximum acceptable processing time for a two-hour recording on the local transcription path without a GPU
- application-update and signed-adapter-manifest distribution

### Resolve before Phase 2

- default retention policy

## 9. MVP completion criteria

The MVP is complete when:

1. The four target meeting services can be detected and recorded on Windows 11 without joining as a bot, with Google Meet relying on the detection-only browser extension.
2. Recording starts and stops automatically with visible user control.
3. A two-hour meeting can be recorded and recovered safely.
4. Transcription, diarization, summary, decisions, and action items can be generated.
5. Results can be stored locally and exported to Google Drive or Notion.
6. The complete workflow runs without a proprietary workflow backend.
7. Meeting-service UI changes do not affect the core detection path, verified across the application versions in the Phase 5 matrix.
8. Network loss, provider failure, and application restart do not lose the meeting or workflow state.
