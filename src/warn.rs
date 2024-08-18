use crate::opts;
use anyhow::{bail, Result};

pub fn warn(msg: &str) -> Result<()> {
    if opts::get().deny_warnings {
        bail!("{msg}");
    }
    eprintln!("Warning: {msg}");
    Ok(())
}
