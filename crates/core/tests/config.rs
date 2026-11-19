use std::path::PathBuf;

use meeting_assistant_core::{CONFIG_VERSION, CaptureScope, Config, ConfigError};

#[test]
fn writing_the_default_settings_and_reading_them_back_gives_the_same_content() {
    let text = Config::default_toml();

    assert_eq!(Config::from_toml(&text).unwrap(), Config::default());
}

#[test]
fn default_settings_output_starts_with_an_explanatory_comment() {
    let text = Config::default_toml();

    assert!(text.starts_with("# Meeting Assistant settings"));
    assert!(text.contains("# output_directory ="));
    assert!(
        text.contains(&format!("\nversion = {CONFIG_VERSION}\n")),
        "the file names its version: {text}"
    );
}

#[test]
fn a_file_that_names_no_version_or_another_one_is_not_read() {
    let found = |text: &str| match Config::from_toml(text) {
        Err(ConfigError::UnsupportedVersion { found }) => found,
        other => panic!("{text:?} gave {other:?}"),
    };

    assert_eq!(found(""), None);
    assert_eq!(found("end_grace_seconds = 30"), None);
    assert_eq!(found("version = \"1\""), None, "not a number");
    assert_eq!(found("version = 2"), Some(2));
    assert_eq!(found("version = 0"), Some(0));
}

/// The settings file the app wrote before formats were numbered, with keys that were later removed.
#[test]
fn a_file_written_before_formats_were_numbered_is_not_read() {
    let written_on_2026_11_17 = r#"
start_debounce_seconds = 3.0
end_grace_seconds = 15.0
poll_interval_milliseconds = 1000
capture_scope = "app"

[[rules]]
name = "Zoom"
process_names = ["zoom"]
require_window_title = false
window_title_patterns = []
"#;

    assert!(matches!(
        Config::from_toml(written_on_2026_11_17),
        Err(ConfigError::UnsupportedVersion { found: None })
    ));
}

#[test]
fn default_settings_have_desktop_app_rules_and_no_browser_rule() {
    let config = Config::default();

    assert!(config.rules.iter().any(|r| r.name == "Zoom"));
    // Browser meetings are detected by the extension only.
    assert!(
        config
            .rules
            .iter()
            .all(|r| !r.process_names.iter().any(|n| n == "chrome" || n == "msedge"))
    );
}

#[test]
fn edited_values_are_loaded_and_omitted_values_become_defaults() {
    let config = Config::from_toml(
        r#"
        # Comments are allowed
        version = 1
        output_directory = 'D:\Recordings'
        end_grace_seconds = 30

        [[rules]]
        name = "Custom"
        process_names = ["myapp.exe"]
        "#,
    )
    .unwrap();

    assert_eq!(config.output_directory, Some(PathBuf::from(r"D:\Recordings")));
    assert_eq!(config.end_grace_seconds, 30.0);
    assert_eq!(config.start_debounce_seconds, 3.0);
    assert_eq!(config.browser_start_debounce_seconds, 1.0);
    assert_eq!(config.browser_end_grace_seconds, 1.0);
    assert_eq!(config.rules.len(), 1);
    assert_eq!(config.rules[0].name, "Custom");
}

#[test]
fn a_file_with_only_its_version_gives_all_defaults() {
    assert_eq!(Config::from_toml("version = 1").unwrap(), Config::default());
}

#[test]
fn the_picture_is_1080p_unless_a_smaller_size_is_set() {
    let default = Config::default();
    assert_eq!((default.video.width, default.video.height), (1920, 1080));
    let smaller = Config::from_toml("version = 1\n[video]\nwidth = 1280\nheight = 720").unwrap();
    assert_eq!((smaller.video.width, smaller.video.height), (1280, 720));
}

#[test]
fn invalid_content_of_this_version_is_a_config_error() {
    assert!(matches!(
        Config::from_toml("not toml = = ="),
        Err(ConfigError::Parse(_))
    ));
    for content in [
        "end_grace_seconds = -1",
        "browser_end_grace_seconds = -1",
        "browser_start_debounce_seconds = -0.5",
        "poll_interval_milliseconds = 10",
        "rules = []",
        "unknown_key = 1",
        "capture_scope = \"tab\"",
        "[video]\nframerate = 0",
        // Above 1080p, below the encoder minimum, or odd.
        "[video]\nwidth = 3840\nheight = 2160",
        "[video]\nwidth = 2560",
        "[video]\nheight = 1440",
        "[video]\nwidth = 160",
        "[video]\nwidth = 1279",
        // Window-title matching for browsers without the extension no longer exists.
        "[[rules]]\nname = \"Bad\"\nprocess_names = [\"x\"]\nrequire_window_title = true",
        "[[rules]]\nname = \"Bad\"\nprocess_names = []",
    ] {
        let versioned = format!("version = 1\n{content}");
        let result = Config::from_toml(&versioned);
        assert!(
            result.is_err() && !matches!(result, Err(ConfigError::UnsupportedVersion { .. })),
            "{versioned}"
        );
    }
}

#[test]
fn unparseable_content_and_invalid_values_are_distinguished_by_error_kind() {
    assert!(matches!(
        Config::from_toml("version = 1\nend_grace_seconds = \"abc\""),
        Err(ConfigError::Parse(_))
    ));
    assert!(matches!(
        Config::from_toml("version = 1\nend_grace_seconds = -1"),
        Err(ConfigError::Invalid(_))
    ));
    assert!(matches!(
        Config::from_toml("version = 1\n[[rules]]\nname = \"Bad\"\nprocess_names = []"),
        Err(ConfigError::InvalidRule { .. })
    ));
}

#[test]
fn recording_scope_defaults_to_app_only_and_can_be_changed_to_whole_display() {
    assert_eq!(Config::default().capture_scope, CaptureScope::App);
    assert_eq!(
        Config::from_toml("version = 1\ncapture_scope = \"screen\"")
            .unwrap()
            .capture_scope,
        CaptureScope::Screen
    );
}
