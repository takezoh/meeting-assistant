//! Reading and writing the config file. Interpreting and validating the contents is done by core.

use std::path::{Path, PathBuf};

use anyhow::Context;
use meeting_assistant_core::{Config, ConfigError};

/// The configuration that was read.
#[derive(Debug)]
pub struct Loaded {
    pub config: Config,
    /// Set when the file was for another configuration version and was replaced with the defaults.
    pub replaced: Option<Replaced>,
}

/// A file for another configuration version, moved aside.
#[derive(Debug, PartialEq, Eq)]
pub struct Replaced {
    /// The version the file named, if it named one as a number.
    pub found: Option<i64>,
    /// Where the file was kept.
    pub kept_at: PathBuf,
}

/// Reads the config. If the file does not exist, it is created with the defaults. If it is for another configuration
/// version, it is kept beside the new file and the defaults are written in its place: without a configuration that
/// it can read, the app would not start, and could not be configured either.
///
/// A file of this version that cannot be parsed or is invalid is an error, and is left as it is for the user to fix.
pub fn load_or_create(path: &Path) -> anyhow::Result<Loaded> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_defaults(path)?;
        return Ok(Loaded {
            config: Config::default(),
            replaced: None,
        });
    }

    let text = std::fs::read_to_string(path).with_context(|| format!("cannot read config file: {}", path.display()))?;
    match Config::from_toml(&text) {
        Ok(config) => Ok(Loaded { config, replaced: None }),
        Err(ConfigError::UnsupportedVersion { found }) => {
            let kept_at = kept_path(path, found);
            std::fs::rename(path, &kept_at)
                .with_context(|| format!("cannot move the config file aside: {}", path.display()))?;
            write_defaults(path)?;
            Ok(Loaded {
                config: Config::default(),
                replaced: Some(Replaced { found, kept_at }),
            })
        }
        Err(error) => Err(error).with_context(|| format!("config file has a problem: {}", path.display())),
    }
}

fn write_defaults(path: &Path) -> anyhow::Result<()> {
    std::fs::write(path, Config::default_toml())
        .with_context(|| format!("cannot create config file: {}", path.display()))
}

/// `config.toml.v2.bak` for a file of version 2, `config.toml.unversioned.bak` for one that names none.
fn kept_path(path: &Path, found: Option<i64>) -> PathBuf {
    let label = found.map_or_else(|| "unversioned".to_owned(), |version| format!("v{version}"));
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{label}.bak"));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_file_with_defaults_when_missing() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("nested").join("config.toml");

        let loaded = load_or_create(&path).unwrap();

        assert_eq!(loaded.config, Config::default());
        assert_eq!(loaded.replaced, None);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), Config::default_toml());
    }

    #[test]
    fn rereading_a_created_file_gives_the_same_content() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");

        assert_eq!(
            load_or_create(&path).unwrap().config,
            load_or_create(&path).unwrap().config
        );
    }

    #[test]
    fn loads_edited_content() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        std::fs::write(&path, "version = 1\nend_grace_seconds = 30").unwrap();

        assert_eq!(load_or_create(&path).unwrap().config.end_grace_seconds, 30.0);
    }

    #[test]
    fn a_file_for_another_version_is_kept_aside_and_replaced_with_the_defaults() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        let old = "end_grace_seconds = 30\n[[rules]]\nname = \"Zoom\"\nprocess_names = [\"zoom\"]\nrequire_window_title = false\n";
        std::fs::write(&path, old).unwrap();

        let loaded = load_or_create(&path).unwrap();

        assert_eq!(loaded.config, Config::default());
        let kept_at = root.path().join("config.toml.unversioned.bak");
        assert_eq!(
            loaded.replaced,
            Some(Replaced {
                found: None,
                kept_at: kept_at.clone()
            })
        );
        assert_eq!(
            std::fs::read_to_string(&kept_at).unwrap(),
            old,
            "the old file is kept as it was"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), Config::default_toml());

        std::fs::write(&path, "version = 2\n").unwrap();
        let loaded = load_or_create(&path).unwrap();
        assert_eq!(
            loaded.replaced.map(|replaced| replaced.kept_at),
            Some(root.path().join("config.toml.v2.bak"))
        );
    }

    #[test]
    fn problematic_file_of_this_version_is_an_error_with_location_and_reason() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        std::fs::write(&path, "version = 1\nend_grace_seconds = -1").unwrap();

        let error = format!("{:#}", load_or_create(&path).unwrap_err());

        assert!(error.contains("config.toml"), "{error}");
        assert!(error.contains("end_grace_seconds"), "{error}");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "version = 1\nend_grace_seconds = -1",
            "the original file is not rewritten"
        );
    }
}
