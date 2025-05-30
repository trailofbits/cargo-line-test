use super::read;
use crate::{opts, run, util, warn, PackageCrateMap, Test, CTRLC};
use anyhow::{anyhow, bail, ensure, Context, Result};
use cargo_metadata::MetadataCommand;
use lcov::{Reader, Record};
use std::{
    collections::{BTreeMap, BTreeSet},
    env::current_dir,
    fs::{create_dir, write},
    io::{BufRead, BufReader},
    os::unix::ffi::OsStrExt,
    path::Path,
    process::{Command, Stdio},
    sync::atomic::Ordering,
};

mod restorer;
use restorer::Restorer;

const README: &str = "\
This directory and its contents were automatically generated by cargo-line-test.
";

pub(crate) fn build() -> Result<()> {
    let mut restorer = None;
    let path = Path::new("line-test.db");

    warn_if_db_not_ignored(path)?;

    #[allow(clippy::collapsible_else_if)]
    if path.try_exists()? {
        if !opts::get().missing_only {
            restorer = save_existing_db(path).map(Some)?;
        }
    } else {
        if opts::get().missing_only {
            bail!("line-test.db does not exist");
        }
    }

    debug_assert_eq!(path.try_exists()?, opts::get().missing_only);

    if !path.try_exists()? {
        create_dir(path)?;
        write(path.join("README.txt"), README)?;
    }

    let mut package_crate_test_map = package_crate_test_map()?;

    if opts::get().missing_only {
        remove_tests_with_lcov(&mut package_crate_test_map)?;
    }

    run::run_tests(&package_crate_test_map, true)?;

    build_digests()?;

    if let Some(restorer) = restorer.as_mut() {
        restorer.disable();
    }

    Ok(())
}

fn warn_if_db_not_ignored(path: &Path) -> Result<()> {
    let mut command = Command::new("git");
    command.args(["check-ignore", &path.to_string_lossy()]);
    let status = command.status()?;
    if !status.success() {
        warn(&format!(
            "{} is not ignored by git, which may cause unnecessary recompilations",
            path.display(),
        ))?;
    }
    Ok(())
}

fn save_existing_db(path: &Path) -> Result<Restorer> {
    eprintln!("saving existing line-test.db; pressing ctrl-c will restore it");

    ctrlc::set_handler(|| CTRLC.store(true, Ordering::SeqCst))?;

    Restorer::new(path)
}

fn package_crate_test_map() -> Result<PackageCrateMap<Vec<Test>>> {
    let package_crates = package_crates()?;

    let mut test_map = PackageCrateMap::<Vec<Test>>::default();
    for (package, crates) in package_crates {
        let test_map = test_map.entry(package.clone()).or_default();
        for krate in crates.keys() {
            let tests = package_crate_tests(&package, krate)?;
            test_map.insert(krate.clone(), tests);
        }
    }

    Ok(test_map)
}

fn package_crates() -> Result<PackageCrateMap<()>> {
    let metadata = MetadataCommand::new().no_deps().exec()?;
    let mut package_crates = PackageCrateMap::default();
    for package in metadata.packages {
        for target in package.targets {
            let krate = if target.is_bin() {
                Some(format!("bin:{}", target.name))
            } else if target.is_lib() {
                Some(String::from("lib"))
            } else if target.is_test() {
                Some(target.name)
            } else {
                None
            };
            if let Some(krate) = krate {
                package_crates
                    .entry(package.name.clone())
                    .or_default()
                    .insert(krate, ());
            }
        }
    }
    Ok(package_crates)
}

// smoelius: Based on:
// https://github.com/trailofbits/test-fuzz/blob/f4f14f0b323cc8457b6a3c6d0187fadb0e477628/cargo-test-fuzz/src/lib.rs#L442-L467

#[cfg_attr(dylint_lib = "supplementary", allow(commented_out_code))]
fn package_crate_tests(package: &str, krate: &str) -> Result<Vec<Test>> {
    let mut command = run::cargo_command(package, krate, None);
    // smoelius: For now, the outputs of the commands to build the tests are shown, which I think I
    // prefer.
    // command.arg("--quiet");
    command.args(["--", "--list", "--format=terse"]);
    command.stdout(Stdio::piped());
    let mut child = command.spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("failed to get child's stdout: {command:?}"))?;

    let mut paths = Vec::new();
    for result in BufReader::new(stdout).lines() {
        let line = result.with_context(|| format!("failed to read child's stdout: {command:?}"))?;
        let Some(path) = line.strip_suffix(": test") else {
            continue;
        };
        paths.push(path.to_owned());
    }

    let status = child.wait()?;
    ensure!(status.success(), "command failed: {command:?}");

    Ok(paths
        .into_iter()
        .map(|path| path.split("::").map(ToOwned::to_owned).collect())
        .collect())
}

fn remove_tests_with_lcov(package_crate_test_map: &mut PackageCrateMap<Vec<Test>>) -> Result<()> {
    let path = Path::new("line-test.db/packages");
    for (package, crate_test_map) in package_crate_test_map {
        let path_buf = path.join(package);
        for (krate, tests) in crate_test_map {
            let path_buf = path_buf.join(krate);
            let mut index = 0;
            while index < tests.len() {
                let test = &tests[index];
                let path_buf = path_buf.join(test.to_string()).with_extension("lcov");
                if path_buf.try_exists()? {
                    tests.remove(index);
                } else {
                    index += 1;
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn build_digests() -> Result<()> {
    let package_crate_test_map = read::read_package_crate_test_map()?;

    let paths = collect_paths(&package_crate_test_map)?;

    let mut path_digest_map = BTreeMap::new();
    for path in paths {
        let digest = util::hash_path_contents(&path)?;
        path_digest_map.insert(path, hex::encode(digest));
    }

    let json = serde_json::to_string_pretty(&path_digest_map)?;
    write("line-test.db/digests.json", json)?;

    Ok(())
}

fn collect_paths(package_crate_test_map: &PackageCrateMap<Vec<Test>>) -> Result<BTreeSet<String>> {
    let mut paths = BTreeSet::new();
    for (package, crate_test_map) in package_crate_test_map {
        for (krate, tests) in crate_test_map {
            for test in tests {
                let path_buf = Path::new("line-test.db/packages")
                    .join(package)
                    .join(krate)
                    .join(test.to_string())
                    .with_extension("lcov");
                ingest_lcov_paths(&mut paths, &path_buf)?;
            }
        }
    }
    Ok(paths)
}

#[allow(clippy::single_match)]
fn ingest_lcov_paths(paths: &mut BTreeSet<String>, path: &Path) -> Result<()> {
    let current_dir = current_dir()?;
    for result in Reader::open_file(path)? {
        match result? {
            Record::SourceFile { path } => {
                let path = path.strip_prefix(&current_dir)?;
                let path_utf8 = String::from_utf8(path.as_os_str().as_bytes().to_owned())?;
                paths.insert(path_utf8);
            }
            _ => {}
        }
    }
    Ok(())
}
