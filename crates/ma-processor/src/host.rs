//! What crosses the host-child boundary: one request frame in, zero or more progress frames and
//! exactly one result frame out (JSON lines — discretion-processor-host-framing); the argv template
//! with typed, whole-argument substitution; secrets by environment or stdin only; the stall watch and
//! the exit classification.

use crate::failure::{Failure, RetryCause};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const HOST_MEMORY_CAP_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// Five times the 30 s item budget.
pub const STALL_TIMEOUT_MS: u64 = 150_000;
pub const GRACEFUL_CANCEL_MS: u64 = 5_000;

/// A parameter type from the signed manifest. Values are validated against it, never interpreted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParamType {
    Int {
        min: i64,
        max: i64,
    },
    Enum {
        values: Vec<String>,
    },
    /// A path inside the staged directory: a bare file name.
    StagedFile,
    Bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParamValue {
    Int(i64),
    Bool(bool),
    Text(String),
}

/// `["--threads", "{threads}", "{input}"]`: a placeholder is exactly one whole argument.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArgvTemplate {
    pub program: String,
    pub args: Vec<String>,
    pub params: BTreeMap<String, ParamType>,
}

/// A secret bound for the child's environment or stdin. No `Display`, no `Serialize`, no `Clone`,
/// `Debug` is `***`, and the buffer is zeroized on drop like `ma_secure::Secret`.
#[derive(zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct SecretValue(Vec<u8>);

impl SecretValue {
    pub fn new(bytes: impl Into<Vec<u8>>) -> SecretValue {
        SecretValue(bytes.into())
    }
    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("***")
    }
}

/// Everything the supervisor needs to spawn the host child. Secrets are not in `argv`.
#[derive(Debug)]
pub struct ChildSpec {
    pub program: String,
    pub argv: Vec<String>,
    pub secret_env: Vec<(String, SecretValue)>,
    pub secret_stdin: Option<SecretValue>,
    pub memory_cap_bytes: u64,
}

impl ChildSpec {
    pub fn new(program: &str, argv: Vec<String>) -> ChildSpec {
        ChildSpec {
            program: program.to_string(),
            argv,
            secret_env: Vec::new(),
            secret_stdin: None,
            memory_cap_bytes: HOST_MEMORY_CAP_BYTES,
        }
    }
    /// The only ways a secret reaches the child.
    pub fn with_secret_env(mut self, name: &str, secret: SecretValue) -> ChildSpec {
        self.secret_env.push((name.to_string(), secret));
        self
    }
    pub fn with_secret_stdin(mut self, secret: SecretValue) -> ChildSpec {
        self.secret_stdin = Some(secret);
        self
    }
    /// The command line another process could read.
    pub fn visible_command_line(&self) -> Vec<String> {
        let mut out = vec![self.program.clone()];
        out.extend(self.argv.iter().cloned());
        out
    }
}

/// Substitute typed values into the template as whole arguments. A value that does not match its
/// declared type is rejected; a value that matches is one literal argument, whatever it contains.
pub fn build_argv(
    template: &ArgvTemplate,
    values: &BTreeMap<String, ParamValue>,
) -> Result<Vec<String>, Failure> {
    let mut out = Vec::with_capacity(template.args.len());
    for arg in &template.args {
        // a placeholder may stand alone ("{input}") or be embedded in a fixed prefix ("--translate={translate}");
        // either way the substituted value is one literal argument
        let mut rendered = String::new();
        let mut rest = arg.as_str();
        while let Some(open) = rest.find('{') {
            let Some(close) = rest[open..].find('}') else {
                return Err(Failure::InvalidInput {
                    reason: "unterminated placeholder in argv template".into(),
                });
            };
            let name = &rest[open + 1..open + close];
            let ty = template
                .params
                .get(name)
                .ok_or_else(|| Failure::InvalidInput {
                    reason: format!("placeholder {name} is not a declared parameter"),
                })?;
            let value = values.get(name).ok_or_else(|| Failure::InvalidInput {
                reason: format!("parameter {name} has no value"),
            })?;
            rendered.push_str(&rest[..open]);
            rendered.push_str(&render(name, ty, value)?);
            rest = &rest[open + close + 1..];
        }
        rendered.push_str(rest);
        out.push(rendered);
    }
    Ok(out)
}

fn render(name: &str, ty: &ParamType, value: &ParamValue) -> Result<String, Failure> {
    let reject = |what: &str| Failure::InvalidInput {
        reason: format!("parameter {name}: {what}"),
    };
    match (ty, value) {
        (ParamType::Int { min, max }, ParamValue::Int(i)) => {
            if i < min || i > max {
                return Err(reject("out of range"));
            }
            Ok(i.to_string())
        }
        (ParamType::Bool, ParamValue::Bool(b)) => Ok(b.to_string()),
        (ParamType::Enum { values }, ParamValue::Text(s)) => {
            if values.iter().any(|v| v == s) {
                Ok(s.clone())
            } else {
                Err(reject("not one of the enumerated values"))
            }
        }
        (ParamType::StagedFile, ParamValue::Text(s)) => {
            let ok = !s.is_empty()
                && !s.contains('/')
                && !s.contains('\\')
                && !s.contains(':')
                && s != "."
                && s != ".."
                && !s.contains("..")
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-');
            if ok {
                Ok(s.clone())
            } else {
                Err(reject("not a bare staged file name"))
            }
        }
        _ => Err(reject("value does not match the declared type")),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestFrame {
    pub job_id: String,
    pub processor_id: String,
    pub staged_dir: String,
    pub argv: Vec<String>,
    pub work_items: u32,
    /// Test scripts for the scripted processor; empty in production.
    #[serde(default)]
    pub script: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressFrame {
    pub completed_items: u32,
    pub total_items: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum ResultFrame {
    Succeeded {
        completed_items: u32,
        output_digest: String,
    },
    Failed {
        failure: Failure,
        completed_items: u32,
    },
}

/// The engine's view of a host child's end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitOutcome {
    /// exit 0 with a well-formed result frame
    Result(ResultFrame),
    /// non-zero exit, abort, access violation, job-object kill, or an unreadable status
    HostCrashed,
    /// killed by the supervisor after the stall timeout; completed items preserved
    NoProgress { completed_items: u32 },
}

/// Classify a child's end. An unreadable status is `HostCrashed`, never success.
pub fn classify_exit(exit_code: Option<i32>, result: Option<ResultFrame>) -> ExitOutcome {
    match (exit_code, result) {
        (Some(0), Some(frame)) => ExitOutcome::Result(frame),
        _ => ExitOutcome::HostCrashed,
    }
}

/// Tracks progress frames against the fixed stall timeout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StallWatch {
    last_progress_ms: u64,
    completed_items: u32,
}

impl StallWatch {
    pub fn start(now_ms: u64) -> StallWatch {
        StallWatch {
            last_progress_ms: now_ms,
            completed_items: 0,
        }
    }
    pub fn progress(&mut self, frame: &ProgressFrame, now_ms: u64) {
        self.last_progress_ms = now_ms;
        self.completed_items = self.completed_items.max(frame.completed_items);
    }
    /// The supervisor kills the child and the step is `Retryable{no_progress}` with completed
    /// items preserved — a different outcome from `HostCrashed`.
    pub fn check(&self, now_ms: u64) -> Option<(ExitOutcome, Failure)> {
        if now_ms.saturating_sub(self.last_progress_ms) >= STALL_TIMEOUT_MS {
            Some((
                ExitOutcome::NoProgress {
                    completed_items: self.completed_items,
                },
                Failure::Retryable {
                    after_ms: 1_000,
                    cause: RetryCause::NoProgress,
                },
            ))
        } else {
            None
        }
    }
}
