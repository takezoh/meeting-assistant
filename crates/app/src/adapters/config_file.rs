//! Reading and writing the config file. Interpreting and validating the contents is done by core.

use std::path::Path;

use anyhow::Context;
use meeting_assistant_core::Config;

/// Reads the config. If the file does not exist, it is created with the defaults.
pub fn load_or_create(path: &Path) -> anyhow::Result<Config> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, Config::default_toml())
            .with_context(|| format!("cannot create config file: {}", path.display()))?;
        return Ok(Config::default());
    }

    let text = std::fs::read_to_string(path).with_context(|| format!("cannot read config file: {}", path.display()))?;
    Config::from_toml(&text).with_context(|| format!("config file has a problem: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_file_with_defaults_when_missing() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("nested").join("config.toml");

        let config = load_or_create(&path).unwrap();

        assert_eq!(config, Config::default());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), Config::default_toml());
    }

    #[test]
    fn rereading_a_created_file_gives_the_same_content() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");

        assert_eq!(load_or_create(&path).unwrap(), load_or_create(&path).unwrap());
    }

    #[test]
    fn loads_edited_content() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        std::fs::write(&path, "end_grace_seconds = 30").unwrap();

        assert_eq!(load_or_create(&path).unwrap().end_grace_seconds, 30.0);
    }

    #[test]
    fn problematic_file_is_an_error_with_location_and_reason() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        std::fs::write(&path, "end_grace_seconds = -1").unwrap();

        let error = format!("{:#}", load_or_create(&path).unwrap_err());

        assert!(error.contains("config.toml"), "{error}");
        assert!(error.contains("end_grace_seconds"), "{error}");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "end_grace_seconds = -1",
            "the original file is not rewritten"
        );
    }
}
