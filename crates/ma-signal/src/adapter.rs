//! The adapter seam as a contract (layer L1): the `MeetingAdapter` trait, the table-driven adapter
//! every service crate instantiates from its own `adapter.toml`, and the conformance suite that the
//! four adapter crates run against themselves. Service identifiers live only in the L4 crates' tables.

use crate::Subject;
use serde::Deserialize;
use std::panic::{catch_unwind, AssertUnwindSafe};

/// What kind of subject an adapter recognised.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    /// A process the adapter owns (desktop application or browser executable).
    Process,
    /// A browser tab the adapter owns (extension-authority evidence).
    Tab,
}

/// The evidence an adapter needs before a start decision is determinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Corroboration {
    /// An operating-system microphone-capture fact from a matched process.
    pub microphone: bool,
    /// An extension tab fact from a matched tab.
    pub tab: bool,
}

/// Which surface class the adapter's meetings belong to; the session layer maps it to a mode default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterClass {
    Desktop,
    Browser,
}

pub trait MeetingAdapter {
    fn id(&self) -> &str;
    /// Precedence when two adapters report concurrently active meetings.
    fn evidence_weight(&self) -> u8;
    fn corroboration(&self) -> Corroboration;
    fn matches(&self, subject: &Subject) -> Option<MatchKind>;
}

/// Self-check fixtures an adapter table ships with; the conformance suite runs them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterFixtures {
    #[serde(default)]
    pub positive_process: Vec<String>,
    #[serde(default)]
    pub negative_process: Vec<String>,
    #[serde(default)]
    pub positive_hosts: Vec<String>,
    #[serde(default)]
    pub negative_hosts: Vec<String>,
}

/// The declarative adapter table. One per service crate, parsed from `adapter.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterSpec {
    pub id: String,
    pub class: AdapterClass,
    pub evidence_weight: u8,
    pub corroboration: Corroboration,
    /// Executable image names of the service's own process (desktop class).
    #[serde(default)]
    pub process_images: Vec<String>,
    /// MSIX package family names of the service's own process.
    #[serde(default)]
    pub package_family_names: Vec<String>,
    /// Browser executables whose microphone use corroborates a tab match (browser class).
    #[serde(default)]
    pub browser_images: Vec<String>,
    /// Hostnames (exact or as a parent domain) of the service's meeting tabs.
    #[serde(default)]
    pub tab_hosts: Vec<String>,
    #[serde(default)]
    pub fixtures: AdapterFixtures,
}

/// Table plus match function: the whole of a service adapter.
#[derive(Debug, Clone)]
pub struct TableAdapter {
    spec: AdapterSpec,
}

impl TableAdapter {
    pub fn from_toml(text: &str) -> Result<TableAdapter, String> {
        let spec: AdapterSpec = toml::from_str(text).map_err(|e| e.to_string())?;
        Ok(TableAdapter { spec })
    }
    pub fn spec(&self) -> &AdapterSpec {
        &self.spec
    }
    pub fn class(&self) -> AdapterClass {
        self.spec.class
    }
}

fn eq_ci(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn host_matches(host: &str, pattern: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let pattern = pattern.to_ascii_lowercase();
    if pattern.is_empty() {
        return false;
    }
    if host == pattern {
        return true;
    }
    // a subdomain: a non-empty label, a dot, then the pattern
    match host.strip_suffix(&pattern) {
        Some(prefix) => prefix.len() > 1 && prefix.ends_with('.') && !prefix.starts_with('.'),
        None => false,
    }
}

impl MeetingAdapter for TableAdapter {
    fn id(&self) -> &str {
        &self.spec.id
    }
    fn evidence_weight(&self) -> u8 {
        self.spec.evidence_weight
    }
    fn corroboration(&self) -> Corroboration {
        self.spec.corroboration
    }
    fn matches(&self, subject: &Subject) -> Option<MatchKind> {
        match subject {
            Subject::Process {
                image_name,
                package_family_name,
                ..
            } => {
                let own = self
                    .spec
                    .process_images
                    .iter()
                    .any(|i| eq_ci(i, image_name))
                    || package_family_name.as_deref().is_some_and(|pfn| {
                        self.spec.package_family_names.iter().any(|p| eq_ci(p, pfn))
                    });
                let browser = self
                    .spec
                    .browser_images
                    .iter()
                    .any(|i| eq_ci(i, image_name));
                (own || browser).then_some(MatchKind::Process)
            }
            Subject::Tab { host, .. } => self
                .spec
                .tab_hosts
                .iter()
                .any(|p| host_matches(host, p))
                .then_some(MatchKind::Tab),
            Subject::Device { .. } | Subject::System => None,
        }
    }
}

/// The shared adapter conformance suite. Returns every violated rule; empty means conformant.
pub fn conformance_violations(adapter: &TableAdapter) -> Vec<String> {
    let spec = adapter.spec();
    let mut out = Vec::new();
    let id_ok = !spec.id.is_empty()
        && spec
            .id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !spec.id.starts_with('-');
    if !id_ok {
        out.push("id must be a non-empty lowercase kebab-case identifier".into());
    }
    if spec.evidence_weight == 0 {
        out.push("evidence_weight must be at least 1".into());
    }
    if spec.corroboration.tab && !spec.corroboration.microphone {
        out.push("extension tab evidence alone can never be sufficient: tab corroboration requires microphone corroboration".into());
    }
    match spec.class {
        AdapterClass::Browser => {
            if !spec.corroboration.tab
                || spec.tab_hosts.is_empty()
                || spec.browser_images.is_empty()
            {
                out.push(
                    "a browser-class adapter needs tab corroboration, tab_hosts and browser_images"
                        .into(),
                );
            }
        }
        AdapterClass::Desktop => {
            if spec.process_images.is_empty() && spec.package_family_names.is_empty() {
                out.push(
                    "a desktop-class adapter needs process_images or package_family_names".into(),
                );
            }
        }
    }
    let process = |image: &str| Subject::Process {
        pid: 4242,
        image_name: image.to_string(),
        package_family_name: None,
    };
    let tab = |host: &str| Subject::Tab {
        host: host.to_string(),
        tab_key: "tab-1".to_string(),
    };
    for image in &spec.fixtures.positive_process {
        if adapter.matches(&process(image)) != Some(MatchKind::Process) {
            out.push(format!(
                "positive process fixture #{} did not match",
                spec.fixtures
                    .positive_process
                    .iter()
                    .position(|i| i == image)
                    .unwrap_or(0)
            ));
        }
    }
    for image in &spec.fixtures.negative_process {
        if adapter.matches(&process(image)).is_some() {
            out.push("a negative process fixture matched".into());
        }
    }
    for host in &spec.fixtures.positive_hosts {
        if adapter.matches(&tab(host)) != Some(MatchKind::Tab) {
            out.push("a positive host fixture did not match".into());
        }
    }
    for host in &spec.fixtures.negative_hosts {
        if adapter.matches(&tab(host)).is_some() {
            out.push("a negative host fixture matched".into());
        }
    }
    if spec.fixtures.positive_process.is_empty() && spec.fixtures.positive_hosts.is_empty() {
        out.push("an adapter must ship at least one positive fixture".into());
    }
    if adapter.matches(&Subject::System).is_some()
        || adapter
            .matches(&Subject::Device {
                endpoint_id: "{0.0.1.00000000}".into(),
            })
            .is_some()
    {
        out.push("system and device subjects are never a match".into());
    }
    // hostile inputs: never a panic, never a match
    let hostile = [
        process(""),
        process(&"x".repeat(10_000)),
        process("\u{0}\u{ffff}"),
        tab(""),
        tab(&format!(
            ".{}",
            spec.tab_hosts.first().cloned().unwrap_or_default()
        )),
        tab("evil.example.test"),
        Subject::Process {
            pid: 0,
            image_name: "".into(),
            package_family_name: Some("".into()),
        },
    ];
    for subject in hostile {
        match catch_unwind(AssertUnwindSafe(|| adapter.matches(&subject))) {
            Ok(None) => {}
            Ok(Some(_)) => out.push("a hostile or empty subject matched".into()),
            Err(_) => out.push("matches() panicked on a hostile subject".into()),
        }
    }
    // a subdomain of a declared host matches; a lookalike suffix does not
    if let Some(h) = spec.tab_hosts.first() {
        if adapter.matches(&tab(&format!("sub.{h}"))) != Some(MatchKind::Tab) {
            out.push("a subdomain of a declared host must match".into());
        }
        if adapter.matches(&tab(&format!("not{h}"))).is_some() {
            out.push("a lookalike suffix must not match".into());
        }
    }
    out
}
