//! Browser extension files bundled into the executable.
//! At install time they are written to a location the browser can load from.

/// File name and contents.
pub const EXTENSION_FILES: &[(&str, &str)] = &[
    ("manifest.json", include_str!("../../../extension/manifest.json")),
    ("background.js", include_str!("../../../extension/background.js")),
    ("bridge.js", include_str!("../../../extension/bridge.js")),
    ("hook.js", include_str!("../../../extension/hook.js")),
    ("offscreen.html", include_str!("../../../extension/offscreen.html")),
    ("offscreen.js", include_str!("../../../extension/offscreen.js")),
    ("options.html", include_str!("../../../extension/options.html")),
    ("options.js", include_str!("../../../extension/options.js")),
];

#[cfg(test)]
mod tests {
    use super::EXTENSION_FILES;
    use crate::browser::protocol::HOST_NAME;

    fn content(name: &str) -> &'static str {
        EXTENSION_FILES
            .iter()
            .find(|(file, _)| *file == name)
            .map(|(_, content)| *content)
            .unwrap()
    }

    #[test]
    fn every_file_referenced_by_manifest_is_bundled() {
        let manifest: serde_json::Value = serde_json::from_str(content("manifest.json")).unwrap();
        let mut referenced = vec![
            manifest["background"]["service_worker"].as_str().unwrap().to_owned(),
            manifest["options_page"].as_str().unwrap().to_owned(),
        ];
        for script in manifest["content_scripts"].as_array().unwrap() {
            referenced.extend(
                script["js"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|js| js.as_str().unwrap().to_owned()),
            );
        }

        for file in referenced {
            assert!(
                EXTENSION_FILES.iter().any(|(name, _)| *name == file),
                "{file} is not bundled"
            );
        }
    }

    #[test]
    fn relay_process_name_the_extension_connects_to_matches_the_name_the_app_registers() {
        assert!(content("background.js").contains(&format!("\"{HOST_NAME}\"")));
    }

    #[test]
    fn manifest_contains_the_key_that_fixes_the_extension_id() {
        let manifest: serde_json::Value = serde_json::from_str(content("manifest.json")).unwrap();
        assert!(manifest["key"].as_str().is_some_and(|key| !key.is_empty()));
    }
}
