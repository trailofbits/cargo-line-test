use crate::{opts, progress::Progress, warn, PackageCrateMap, Test, CTRLC};
use anyhow::{bail, ensure, Result};
use assert_cmd::output::OutputError;
use std::{
    cmp::max,
    env::var,
    fs::create_dir_all,
    io::{stderr, IsTerminal},
    path::Path,
    process::Command,
    sync::atomic::Ordering,
};

pub(crate) fn run_tests(
    package_crate_test_map: &PackageCrateMap<Vec<Test>>,
    coverage: bool,
) -> Result<()> {
    let mut package_width = 0;
    let mut crate_width = 0;
    let mut test_width = 0;
    let mut n = 0;

    for (package, crate_test_map) in package_crate_test_map {
        package_width = max(package_width, package.len());
        for (krate, tests) in crate_test_map {
            crate_width = max(crate_width, krate.len());
            for test in tests {
                test_width = max(test_width, test.to_string().len());
            }
            n += tests.len();
        }
    }

    let mut progress = if stderr().is_terminal() && coverage && !opts::get().verbose {
        Some(Progress::new(n))
    } else {
        None
    };

    let path = Path::new("line-test.db/packages");
    for (package, crate_test_map) in package_crate_test_map {
        if CTRLC.load(Ordering::SeqCst) {
            bail!("ctrl-c detected");
        }

        let path_buf = path.join(package);

        for (krate, tests) in crate_test_map {
            if CTRLC.load(Ordering::SeqCst) {
                bail!("ctrl-c detected");
            }

            let path_buf = path_buf.join(krate);

            if tests.is_empty() {
                continue;
            }

            if coverage {
                create_dir_all(&path_buf).unwrap_or_default();
            }

            for test in tests {
                if CTRLC.load(Ordering::SeqCst) {
                    bail!("ctrl-c detected");
                }

                let path_buf = path_buf.join(test.to_string()).with_extension("lcov");

                if let Some(progress) = progress.as_mut() {
                    progress.advance(&format!(
                        "package: {:package_width$}  crate: {:crate_width$}  test: {:test_width$}",
                        package,
                        krate,
                        test.to_string()
                    ))?;
                }

                // smoelius: Passing --no-clean to `cargo llvm-cov` makes successively running tests
                // from the same crate faster. However, it leaves around profraw files, which cause
                // false positive coverage reports. So, remove the profraw files. See:
                // https://github.com/taiki-e/cargo-llvm-cov/pull/385
                if coverage {
                    remove_profraw_files()?;
                }

                let mut command = cargo_command(
                    package,
                    krate,
                    if coverage { Some(&path_buf) } else { None },
                );
                command.args(["--", "--exact", &test.to_string()]);

                if opts::get().show_commands {
                    if let Some(progress) = progress.as_mut() {
                        progress.newline();
                    }
                    println!("{command:?}");
                }

                if opts::get().no_run {
                    continue;
                }

                if opts::get().verbose {
                    let status = command.status()?;
                    if !status.success() {
                        if let Some(progress) = progress.as_mut() {
                            progress.newline();
                        }
                        warn(&format!("command failed: {command:?}"))?;
                    }
                } else {
                    let output = command.output()?;
                    if !output.status.success() {
                        // smoelius: Note that `progress` is necessarily `None` when --verbose is
                        // used.
                        warn(&format!(
                            "command failed: {command:?}\n{}",
                            OutputError::new(output)
                        ))?;
                    }
                }
            }
        }
    }

    if let Some(progress) = progress.as_mut() {
        progress.finish()?;
    }

    Ok(())
}

fn remove_profraw_files() -> Result<()> {
    let mut command = Command::new("cargo");
    command.args(["llvm-cov", "clean", "--profraw-only"]);
    let status = command.status()?;
    ensure!(status.success(), "command failed: {command:?}");
    Ok(())
}

pub(crate) fn cargo_command(package: &str, krate: &str, path: Option<&Path>) -> Command {
    let cargo = var("CARGO").unwrap_or_else(|_| String::from("cargo"));
    let mut command = Command::new(cargo);
    command.arg(if path.is_some() { "llvm-cov" } else { "test" });
    command.args(["--package", package]);
    command.args(test_selection(krate));
    if let Some(path) = path {
        command.args([
            "--no-clean",
            "--lcov",
            "--output-path",
            &path.to_string_lossy(),
            // "-vv",
        ]);
    }
    command.args(&opts::get().zzargs);
    command
}

// smoelius: This doesn't have an appreciable effect on performance, and it complicates the output
// of --show-commands.
#[cfg(any())]
static CARGO: once_cell::sync::Lazy<String> = once_cell::sync::Lazy::new(|| {
    let mut command = Command::new("rustup");
    command.args(["which", "cargo"]);
    let output = command.output().unwrap();
    assert!(output.status.success(), "command failed: {command:?}");
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    stdout.trim_end().to_owned()
});

pub(crate) fn test_selection(krate: &str) -> Vec<&str> {
    if krate == "lib" {
        vec!["--lib"]
    } else if let Some(bin) = krate.strip_prefix("bin:") {
        vec!["--bin", bin]
    } else {
        vec!["--test", krate]
    }
}
