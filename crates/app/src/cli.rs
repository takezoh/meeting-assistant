//! Command-line parsing. Has no side effects.

use std::path::PathBuf;

use crate::browser::protocol::extension_origin;

pub const USAGE: &str = "\
Usage: meeting-assistant [--config <path>] [<command>]

  (no command)         Stay resident, detect meetings and record them
  --install            Install (copy files, start at logon, link with the browser, browser shortcuts)
  --uninstall          Uninstall (the config file, logs, and recordings are kept)
  --quit               Ask the running resident app to finalize its recording and exit
  --probe              Show the current signals and the verdict of each rule
  --self-test [<path>] Check that a 5-second recording works (keeps the recording if <path> is given)

  --config <path>      Set the config file location (logs go to the logs folder next to it)
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Stay resident.
    Run,
    Install,
    Uninstall,
    Quit,
    Probe,
    /// Recording self-test. If a location is given, the recording is kept there.
    SelfTest(Option<PathBuf>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    /// Launched by the browser as the extension's relay process.
    NativeHost,
    Command {
        config: Option<PathBuf>,
        command: Command,
    },
}

/// The arguments could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageError;

/// Parses the arguments, excluding the program name.
pub fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Invocation, UsageError> {
    let mut rest = arguments.into_iter().peekable();
    // When the browser starts the relay process, it passes the extension's origin as the first argument.
    if rest.peek().is_some_and(|first| *first == extension_origin()) {
        return Ok(Invocation::NativeHost);
    }

    let mut config = None;
    let mut command = None;
    while let Some(argument) = rest.next() {
        let parsed = match argument.as_str() {
            "--config" => {
                config = Some(PathBuf::from(rest.next().ok_or(UsageError)?));
                continue;
            }
            "--install" => Command::Install,
            "--uninstall" => Command::Uninstall,
            "--quit" => Command::Quit,
            "--probe" => Command::Probe,
            "--self-test" => Command::SelfTest(rest.next_if(|next| !next.starts_with("--")).map(PathBuf::from)),
            _ => return Err(UsageError),
        };
        // Only one command is allowed.
        if command.replace(parsed).is_some() {
            return Err(UsageError);
        }
    }
    Ok(Invocation::Command {
        config,
        command: command.unwrap_or(Command::Run),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_words(words: &[&str]) -> Result<Invocation, UsageError> {
        parse(words.iter().map(|word| word.to_string()))
    }

    fn command(command: Command) -> Result<Invocation, UsageError> {
        Ok(Invocation::Command { config: None, command })
    }

    #[test]
    fn stays_resident_without_arguments() {
        assert_eq!(parse_words(&[]), command(Command::Run));
    }

    #[test]
    fn parses_commands() {
        assert_eq!(parse_words(&["--install"]), command(Command::Install));
        assert_eq!(parse_words(&["--uninstall"]), command(Command::Uninstall));
        assert_eq!(parse_words(&["--quit"]), command(Command::Quit));
        assert_eq!(parse_words(&["--probe"]), command(Command::Probe));
        assert_eq!(parse_words(&["--self-test"]), command(Command::SelfTest(None)));
    }

    #[test]
    fn self_test_accepts_a_location_to_keep_the_recording() {
        assert_eq!(
            parse_words(&["--self-test", "out.mp4"]),
            command(Command::SelfTest(Some("out.mp4".into())))
        );
    }

    #[test]
    fn config_location_can_come_before_or_after_the_command() {
        let expected = Ok(Invocation::Command {
            config: Some("c.toml".into()),
            command: Command::Probe,
        });

        assert_eq!(parse_words(&["--config", "c.toml", "--probe"]), expected);
        assert_eq!(parse_words(&["--probe", "--config", "c.toml"]), expected);
        assert_eq!(
            parse_words(&["--self-test", "--config", "c.toml"]),
            Ok(Invocation::Command {
                config: Some("c.toml".into()),
                command: Command::SelfTest(None)
            })
        );
    }

    #[test]
    fn stays_resident_when_only_config_location_is_given() {
        assert_eq!(
            parse_words(&["--config", "c.toml"]),
            Ok(Invocation::Command {
                config: Some("c.toml".into()),
                command: Command::Run
            })
        );
    }

    #[test]
    fn unparsable_arguments_are_a_usage_error() {
        assert_eq!(parse_words(&["--bogus"]), Err(UsageError));
        assert_eq!(parse_words(&["--config"]), Err(UsageError), "missing location");
        assert_eq!(parse_words(&["--probe", "--quit"]), Err(UsageError), "two commands");
        assert_eq!(parse_words(&["stray"]), Err(UsageError));
    }

    #[test]
    fn runs_as_relay_process_when_browser_passes_the_extension_origin() {
        let origin = extension_origin();

        assert_eq!(parse_words(&[&origin]), Ok(Invocation::NativeHost));
        assert_eq!(
            parse_words(&[&origin, "--parent-window=123"]),
            Ok(Invocation::NativeHost)
        );
    }

    #[test]
    fn other_extension_origin_does_not_make_a_relay_process() {
        assert_eq!(
            parse_words(&["chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"]),
            Err(UsageError)
        );
    }
}
