// smoelius: Based on:
// https://github.com/trailofbits/cargo-unmaintained/blob/4a6a4473f04a2dd54173fe6b84958f50ffad7a7d/src/progress.rs

use std::io::Write;

use anyhow::{Context, Result};

pub struct Progress {
    n: usize,
    i: usize,
    width_n: usize,
    width_prev: usize,
    newline_needed: bool,
    finished: bool,
}

impl Drop for Progress {
    fn drop(&mut self) {
        if !self.finished {
            self.finish().unwrap_or_default();
        }
    }
}

impl Progress {
    pub fn new(n: usize) -> Self {
        Self {
            n,
            i: 0,
            width_n: n.to_string().len(),
            width_prev: 0,
            newline_needed: false,
            finished: false,
        }
    }

    pub fn advance(&mut self, msg: &str) -> Result<()> {
        self.draw(msg)?;
        assert!(self.i < self.n);
        self.i += 1;
        Ok(())
    }

    pub fn finish(&mut self) -> Result<()> {
        self.draw("")?;
        self.newline();
        self.finished = true;
        Ok(())
    }

    pub fn newline(&mut self) {
        if self.newline_needed {
            eprintln!();
        }
        self.newline_needed = false;
    }

    fn draw(&mut self, msg: &str) -> Result<()> {
        assert!(self.i < self.n || msg.is_empty());
        let width_n = self.width_n;
        let percent = format!("({}%)", (self.i * 100).checked_div(self.n).unwrap_or(100));
        let formatted_msg = format!("{:>width_n$}/{} {percent:>5} {msg}", self.i, self.n,);
        let width_to_overwrite = self.width_prev.saturating_sub(formatted_msg.len());
        eprint!("{formatted_msg}{:width_to_overwrite$}\r", "");
        std::io::stderr()
            .flush()
            .with_context(|| "failed to flush stderr")?;
        self.width_prev = formatted_msg.len();
        self.newline_needed = true;
        Ok(())
    }
}
