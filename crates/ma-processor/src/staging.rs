//! Per-job staged input directory: exactly the declared inputs, nothing else, removed at job end.
//! The owner-only ACL is applied by the platform layer from the descriptor this returns.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StagingError {
    NotAFile(PathBuf),
    NameCollision(String),
    Io(String),
}

/// A staged directory. Dropping it removes the directory and everything in it.
#[derive(Debug)]
pub struct StagedDir {
    dir: PathBuf,
    names: Vec<String>,
    /// The SID the platform grants exclusive access to.
    pub owner_sid: String,
    owned: bool,
}

impl StagedDir {
    /// Copy each declared input under its file name into a fresh directory under `root`.
    pub fn create(
        root: &Path,
        job_id: &str,
        declared_inputs: &[PathBuf],
        owner_sid: &str,
    ) -> Result<StagedDir, StagingError> {
        let dir = root.join(format!("job-{job_id}"));
        let mut names: Vec<String> = Vec::new();
        for input in declared_inputs {
            if !input.is_file() {
                return Err(StagingError::NotAFile(input.clone()));
            }
            let name = input
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .ok_or_else(|| StagingError::NotAFile(input.clone()))?;
            if names.contains(&name) {
                return Err(StagingError::NameCollision(name));
            }
            names.push(name);
        }
        std::fs::create_dir_all(&dir).map_err(|e| StagingError::Io(e.to_string()))?;
        let staged = StagedDir {
            dir,
            names: names.clone(),
            owner_sid: owner_sid.to_string(),
            owned: true,
        };
        for (input, name) in declared_inputs.iter().zip(&names) {
            std::fs::copy(input, staged.dir.join(name))
                .map_err(|e| StagingError::Io(e.to_string()))?;
        }
        Ok(staged)
    }

    /// Use a directory the engine already staged (the host child side). Not removed on drop.
    pub fn adopt(dir: &Path) -> StagedDir {
        let names = std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        StagedDir {
            dir: dir.to_path_buf(),
            names,
            owner_sid: String::new(),
            owned: false,
        }
    }

    pub fn path(&self) -> &Path {
        &self.dir
    }

    /// The names the caller declared, in order.
    pub fn declared(&self) -> &[String] {
        &self.names
    }

    /// What is actually on disk, sorted: the contract test compares this with `declared`.
    pub fn listing(&self) -> Result<Vec<String>, StagingError> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&self.dir).map_err(|e| StagingError::Io(e.to_string()))? {
            let entry = entry.map_err(|e| StagingError::Io(e.to_string()))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.path().is_dir() {
                out.push(format!("{name}/"));
            } else {
                out.push(name);
            }
        }
        out.sort();
        Ok(out)
    }
}

impl Drop for StagedDir {
    fn drop(&mut self) {
        if self.owned {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}
