use std::path::PathBuf;

use meeting_assistant_core::{CaptureScope, Config, ConfigError};

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
fn an_empty_string_gives_all_defaults() {
    assert_eq!(Config::from_toml("").unwrap(), Config::default());
}

#[test]
fn the_picture_is_1080p_unless_a_smaller_size_is_set() {
    let default = Config::default();
    assert_eq!((default.video.width, default.video.height), (1920, 1080));
    let smaller = Config::from_toml("[video]\nwidth = 1280\nheight = 720").unwrap();
    assert_eq!((smaller.video.width, smaller.video.height), (1280, 720));
}

#[test]
fn invalid_content_is_a_config_error() {
    for content in [
        "not toml = = =",
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
        assert!(Config::from_toml(content).is_err(), "{content}");
    }
}

#[test]
fn unparseable_content_and_invalid_values_are_distinguished_by_error_kind() {
    assert!(matches!(
        Config::from_toml("end_grace_seconds = \"abc\""),
        Err(ConfigError::Parse(_))
    ));
    assert!(matches!(
        Config::from_toml("end_grace_seconds = -1"),
        Err(ConfigError::Invalid(_))
    ));
    assert!(matches!(
        Config::from_toml("[[rules]]\nname = \"Bad\"\nprocess_names = []"),
        Err(ConfigError::InvalidRule { .. })
    ));
}

#[test]
fn recording_scope_defaults_to_app_only_and_can_be_changed_to_whole_display() {
    assert_eq!(Config::default().capture_scope, CaptureScope::App);
    assert_eq!(
        Config::from_toml("capture_scope = \"screen\"").unwrap().capture_scope,
        CaptureScope::Screen
    );
}
