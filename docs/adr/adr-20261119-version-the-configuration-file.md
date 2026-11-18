---
id: adr-20261119-version-the-configuration-file
kind: adr
title: Version the configuration file and replace a file of another version with the
  defaults
status: proposed
created: '2026-11-19'
decision_makers:
- user
tags: []
owners: []
relations:
- {type: references, target: adr-20261118-detect-browser-meetings-only-through-the}
- {type: references, target: design-meeting-detection}
source_paths:
- crates/core/src/config.rs
- crates/app/src/adapters/config_file.rs
- crates/app/src/main.rs
summary: The settings file names its format version; a file of another version, including
  one written before versions existed, is kept aside and replaced with the defaults
  at startup. No older format is converted.
---

# Version the configuration file and replace a file of another version with the defaults

## Context

The configuration rejects unknown and removed keys at startup rather than ignoring them ([meeting detection design](../design/design-meeting-detection.md), COMPAT-001), and updating the app leaves user data untouched ([installation design](../design/design-installation.md), COMPAT-001). When the window-title rules were removed ([ADR](adr-20261118-detect-browser-meetings-only-through-the.md)), the two together meant that an update stopped the app at startup for anyone whose file still had those keys. On 2026-11-19 that happened on the developer's PC to a file the app itself had written with its defaults on 2026-11-17: the updated app showed an error dialog and did not start, and the installer had reported it started. Nothing told the configuration of one version from a typo, so the app could only report an unknown key.

The user decided: no backward compatibility; the file names its version, the app checks that it supports it, and a file of a version it does not support is reset to the defaults, because an app that does not start cannot be configured either.

## Decision

The file carries `version`, the format of the configuration (`CONFIG_VERSION`, now 1), and the defaults the app writes name it. At startup `Config::from_toml` reads the version before anything else. A file that names another version, or none, is not read: the file adapter moves it beside the new file (`config.toml.v<version>.bak`, or `config.toml.unversioned.bak`), writes the defaults in its place, and the app starts with them. The log says so, and the resident app also shows a notification naming the kept file.

`CONFIG_VERSION` is raised whenever a file of the current format would read differently or not at all: a key removed, renamed or given another meaning. Adding a key with a default does not raise it. No older format is converted.

Within the supported version nothing changes: a file that cannot be parsed, has an unknown key or fails validation still stops the app with a message naming the file and the reason, and the file is left as it is, so that a typo does not cost the user their settings.

## Consequences

- Positive: an update never leaves the app unable to start because of an older file, including one the app wrote itself.
- Positive: the version tells an older file from a mistake in a current one, so each is handled differently.
- Negative: settings made under an older version are not carried over; the user re-applies them from the kept file.
- Neutral: a file without `version` is treated as an older one, so a hand-written file must name the version.

## Confirmation

`crates/core/tests/config.rs` asserts that a file naming no version, another version or a version that is not a number is refused as unsupported, including the file the app wrote on 2026-11-17, and that errors in a file of this version are not. `crates/app/src/adapters/config_file.rs` asserts that such a file is kept as it was beside the defaults and that an invalid file of this version is left untouched.
