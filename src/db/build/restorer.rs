use anyhow::{anyhow, Result};
use std::{
    ffi::OsString,
    fs::{remove_dir_all, rename},
    path::{Path, PathBuf},
};
use tempfile::TempDir;

pub struct Restorer {
    canonical_path: PathBuf,
    tempdir: TempDir,
    filename: OsString,
    disabled: bool,
}

impl Restorer {
    pub fn new<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let (canonical_path, tempdir, filename) = sibling_tempdir(path)?;
        rename(&canonical_path, tempdir.path().join(&filename))?;
        Ok(Self {
            canonical_path,
            tempdir,
            filename,
            disabled: false,
        })
    }

    pub fn disable(&mut self) {
        self.disabled = true;
    }
}

impl Drop for Restorer {
    fn drop(&mut self) {
        if self.disabled {
            return;
        }
        remove_dir_all(&self.canonical_path).unwrap_or_default();
        rename(
            self.tempdir.path().join(&self.filename),
            &self.canonical_path,
        )
        .unwrap_or_default();
    }
}

fn sibling_tempdir(path: impl AsRef<Path>) -> Result<(PathBuf, TempDir, OsString)> {
    let canonical_path = path.as_ref().canonicalize()?;
    let parent = canonical_path
        .parent()
        .expect("`parent` should not fail for a canonical path");
    let tempdir = TempDir::new_in(parent)?;
    let filename = canonical_path
        .file_name()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("path has no filename: {}", canonical_path.display()))?;
    Ok((canonical_path, tempdir, filename))
}
