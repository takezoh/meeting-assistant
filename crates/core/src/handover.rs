//! Which files in the in-progress folder belong to one recording, and how they are handed over to the recordings
//! folder once nothing writes them any more. No side effects.
//!
//! A recording is its own file, `<stem>.<extension>` (`.mp4`, or `.webm` for a tab the browser recorded as WebM), and
//! the files written beside it, `<stem>.<suffix>.<extension>`: the microphone recorded beside a tab recording, and the
//! copies the app writes while adding that microphone or the metadata, which replace the recording when complete.

/// Suffix of the microphone recorded beside a tab recording: `<stem>.microphone.m4a`.
pub const MICROPHONE: &str = "microphone";
/// Suffix of the copy written while the microphone is added to a recording: `<stem>.merging.<extension>`.
pub const MERGING: &str = "merging";
/// Suffix of the copy written while the metadata is written into a recording: `<stem>.annotating.<extension>`.
pub const ANNOTATING: &str = "annotating";

/// The name of a file written beside the recording `stem`.
pub fn beside(stem: &str, suffix: &str, extension: &str) -> String {
    format!("{stem}.{suffix}.{extension}")
}

/// The files of one recording found in the in-progress folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// The recording's name without its extension.
    pub stem: String,
    /// The recording's own file, when it is there.
    pub recording: Option<String>,
    /// Every file of the recording, its own included, sorted.
    pub files: Vec<String>,
}

/// One file moved into the recordings folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Move {
    pub from: String,
    pub to: String,
}

/// What to do with a recording's files: remove `discard` from the in-progress folder, then make `moves` in order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Handover {
    pub discard: Vec<String>,
    pub moves: Vec<Move>,
}

impl Handover {
    /// Where the recording's own file goes, when it is moved.
    pub fn recording_target(&self, group: &Group) -> Option<&str> {
        let recording = group.recording.as_deref()?;
        self.moves.iter().find(|m| m.from == recording).map(|m| m.to.as_str())
    }
}

/// `(stem, suffix)` for a file written beside a recording, `None` for a recording's own file.
fn beside_parts(file_name: &str) -> Option<(&str, &str)> {
    let (rest, _extension) = file_name.rsplit_once('.')?;
    let (stem, suffix) = rest.rsplit_once('.')?;
    [MICROPHONE, MERGING, ANNOTATING]
        .contains(&suffix)
        .then_some((stem, suffix))
}

/// The name without its last extension.
pub fn stem_of(file_name: &str) -> &str {
    file_name.rsplit_once('.').map_or(file_name, |(stem, _)| stem)
}

/// The files of the recording `stem` among `names`.
pub fn group(stem: &str, names: &[String]) -> Group {
    let mut files: Vec<String> = names
        .iter()
        .filter(|name| name.strip_prefix(stem).is_some_and(|rest| rest.starts_with('.')))
        .filter(|name| match beside_parts(name) {
            Some((of, _)) => of == stem,
            None => stem_of(name) == stem,
        })
        .cloned()
        .collect();
    files.sort();
    let recording = files.iter().find(|name| beside_parts(name).is_none()).cloned();
    Group {
        stem: stem.to_owned(),
        recording,
        files,
    }
}

/// Every recording in `names`, as the in-progress folder holds them after an earlier run ended without handing them
/// over. A file written beside a recording that is not there forms a group of its own.
pub fn groups(names: &[String]) -> Vec<Group> {
    let mut stems: Vec<&str> = names
        .iter()
        .map(|name| beside_parts(name).map_or_else(|| stem_of(name), |(stem, _)| stem))
        .collect();
    stems.sort_unstable();
    stems.dedup();
    stems.into_iter().map(|stem| group(stem, names)).collect()
}

/// `file` of the recording `stem` renamed for the `attempt`-th name of that recording: the first keeps its name, the
/// second and later insert `_2`, `_3`, ... after the stem, as [`crate::naming::candidate_file_name`] does.
fn renamed(file: &str, stem: &str, attempt: u32) -> String {
    if attempt <= 1 {
        file.to_owned()
    } else {
        format!("{stem}_{attempt}{}", &file[stem.len()..])
    }
}

/// How the files of `group` are handed over to the recordings folder, where `is_taken` tells the names in use.
///
/// - A copy that was replacing the recording is removed when the recording itself is there: it is unfinished, and the
///   recording it was made from is intact. Without the recording it is kept and moved, so that nothing is lost.
/// - The files keep their names unless one of them is taken; then all of them take the first free `_2`, `_3`, ...
///   after the stem, so they stay together.
/// - The recording's own file moves last, so a file named like a recording appears in the recordings folder only
///   after everything written beside it.
pub fn handover(group: &Group, is_taken: impl Fn(&str) -> bool) -> Handover {
    let is_copy =
        |name: &str| group.recording.is_some() && beside_parts(name).is_some_and(|(_, suffix)| suffix != MICROPHONE);
    let discard: Vec<String> = group.files.iter().filter(|name| is_copy(name)).cloned().collect();
    let mut kept: Vec<&String> = group.files.iter().filter(|name| !is_copy(name)).collect();
    kept.sort_by_key(|name| (Some(name.as_str()) == group.recording.as_deref(), name.as_str()));
    let attempt = (1..)
        .find(|&attempt| kept.iter().all(|name| !is_taken(&renamed(name, &group.stem, attempt))))
        .expect("names never run out");
    Handover {
        discard,
        moves: kept
            .into_iter()
            .map(|name| Move {
                from: name.clone(),
                to: renamed(name, &group.stem, attempt),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn a_recording_with_the_files_written_beside_it_forms_one_group() {
        let all = names(&[
            "a.mp4",
            "a.microphone.m4a",
            "a.annotating.mp4",
            "a_2.mp4",
            "b.webm",
            "b.microphone.m4a",
        ]);
        assert_eq!(
            group("a", &all),
            Group {
                stem: "a".into(),
                recording: Some("a.mp4".into()),
                files: names(&["a.annotating.mp4", "a.microphone.m4a", "a.mp4"]),
            }
        );
        assert_eq!(group("b", &all).recording.as_deref(), Some("b.webm"));
        assert_eq!(group("a_2", &all).files, names(&["a_2.mp4"]));
    }

    #[test]
    fn a_rule_name_with_a_dot_stays_one_recording() {
        let all = names(&["x_Web.Meet.mp4", "x_Web.Meet.microphone.m4a", "x_Web.mp4"]);
        let found = groups(&all);
        assert_eq!(found.len(), 2);
        assert_eq!(
            found[1].files,
            names(&["x_Web.Meet.microphone.m4a", "x_Web.Meet.mp4"]),
            "{found:?}"
        );
        assert_eq!(found[0].files, names(&["x_Web.mp4"]));
    }

    #[test]
    fn a_file_written_beside_a_missing_recording_is_a_group_of_its_own() {
        let found = groups(&names(&["a.microphone.m4a"]));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].recording, None);
        assert_eq!(found[0].files, names(&["a.microphone.m4a"]));
    }

    #[test]
    fn the_recording_moves_last_and_unfinished_copies_are_removed() {
        let g = group(
            "a",
            &names(&["a.mp4", "a.microphone.m4a", "a.merging.mp4", "a.annotating.mp4"]),
        );
        let plan = handover(&g, |_| false);
        assert_eq!(plan.discard, names(&["a.annotating.mp4", "a.merging.mp4"]));
        let moved: Vec<(&str, &str)> = plan.moves.iter().map(|m| (m.from.as_str(), m.to.as_str())).collect();
        assert_eq!(moved, [("a.microphone.m4a", "a.microphone.m4a"), ("a.mp4", "a.mp4")]);
        assert_eq!(plan.recording_target(&g), Some("a.mp4"));
    }

    #[test]
    fn a_copy_without_its_recording_is_kept() {
        let g = group("a", &names(&["a.merging.mp4"]));
        let plan = handover(&g, |_| false);
        assert!(plan.discard.is_empty());
        assert_eq!(plan.moves[0].to, "a.merging.mp4");
    }

    #[test]
    fn all_files_take_the_next_free_name_together_when_one_is_taken() {
        let g = group("a", &names(&["a.mp4", "a.microphone.m4a"]));
        let plan = handover(&g, |name| ["a.microphone.m4a", "a_2.mp4"].contains(&name));
        let targets: Vec<&str> = plan.moves.iter().map(|m| m.to.as_str()).collect();
        assert_eq!(targets, ["a_3.microphone.m4a", "a_3.mp4"]);
    }
}
