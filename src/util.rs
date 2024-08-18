use anyhow::Result;
use sha2::{Digest, Sha256};
use std::path::Path;

pub(crate) fn hash_path_contents(path: impl AsRef<Path>) -> Result<[u8; 32]> {
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hasher.finalize().into())
}
