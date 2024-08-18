use anyhow::{anyhow, bail, ensure, Result};
use clap::{crate_version, ArgAction, Parser};
use std::{
    collections::{BTreeMap, HashSet},
    io::{read_to_string, stdin, BufRead, BufReader},
    ops::Range,
    path::Path,
    sync::atomic::AtomicBool,
};
use unidiff::PatchSet;

mod opts;
mod progress;
mod run;

mod db;
use db::Db;

mod util;
use util::hash_path_contents;

mod range_set;
use range_set::RangeSet;

mod warn;
use warn::warn;

type PathLineMap = BTreeMap<String, RangeSet<u32>>;

type PackageCrateMap<T> = BTreeMap<String, CrateMap<T>>;
type CrateMap<T> = BTreeMap<String, T>;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Test(Vec<String>);

impl Test {
    #[allow(dead_code)]
    fn take(&mut self) -> Test {
        Self(self.0.split_off(0))
    }
}

impl std::fmt::Display for Test {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.join("::").fmt(f)
    }
}

impl FromIterator<String> for Test {
    fn from_iter<T: IntoIterator<Item = String>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

type PathCoverageMap = BTreeMap<String, HashSet<u32>>;

type PathDigestMap = BTreeMap<String, [u8; 32]>;

#[derive(Parser)]
#[command(bin_name = "cargo")]
struct CargoCommand {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser)]
enum SubCommand {
    LineTest(Opts),
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Parser)]
#[clap(
    version = crate_version!(),
    about = "Run tests by the lines they exercise",
    after_help = "\
If any <SPEC> is '-', then line specifications are read from standard input. All other <SPEC> \
should adhere to the following syntax:

    <SPEC>:  <PATH> ':' <GROUP>
    <GROUP>: <LINES> (',' <LINES>)* 
    <LINES>: <N> ('-' <N>)?

Example line specification:

    src/main.rs:95-97,99
"
)]
#[remain::sorted]
struct Opts {
    #[clap(
        long,
        help = "Build new line-test.db directory",
        conflicts_with_all = ["diff", "lines", "zero_coverage", "refresh"], // "no_run",
    )]
    build: bool,

    #[clap(long, help = "Treat warnings as errors")]
    deny_warnings: bool,

    #[clap(
        long,
        help = "Generate line specifications from a diff read from standard input"
    )]
    diff: bool,

    #[clap(
        action = ArgAction::Append,
        number_of_values = 1,
        long = "line",
        value_name = "SPEC",
        help = "Line(s) to exercise with tests; can be passed multiple times",
    )]
    lines: Vec<String>,

    #[clap(
        long,
        help = "Build missing line-test.db coverage files only",
        requires = "build"
    )]
    missing_only: bool,

    #[clap(long, help = "Do not run tests; implies --show-commands")]
    no_run: bool,

    #[clap(
        long,
        help = "Update line-test.db coverage for source files that have changed",
        conflicts_with_all = ["diff", "lines", "zero_coverage"],
    )]
    refresh: bool,

    #[clap(long, help = "Show commands that would or will be executed")]
    show_commands: bool,

    #[clap(long, help = "Show command output when computing coverage")]
    verbose: bool,

    #[clap(long, help = "Select tests that have zero coverage")]
    zero_coverage: bool,

    #[clap(
        last = true,
        name = "ARGS",
        help = "Arguments for `cargo test`/`cargo llvm-cov`"
    )]
    zzargs: Vec<String>,
}

static CTRLC: AtomicBool = AtomicBool::new(false);

fn main() -> Result<()> {
    if opts::get().build {
        return db::build();
    }

    if opts::get().refresh {
        return refresh();
    }

    run_tests()
}

fn run_tests() -> Result<()> {
    let (mut path_line_map, line_dash_used) = parse_line_specifications()?;

    if opts::get().diff {
        ensure!(!line_dash_used, "--diff cannot be used with `--line -`");
        let mut other = read_diff()?;
        path_line_map.append(&mut other);
    } else if line_dash_used {
        let mut other = read_line_specifications()?;
        path_line_map.append(&mut other);
    };

    let db = db::read()?;

    validate_paths(&db, &mut path_line_map)?;

    let coverage_map = db.coverage_map()?;

    let mut test_map = tests_for_path_lines(&coverage_map, &path_line_map)?;

    if opts::get().zero_coverage {
        test_map.append(&mut zero_coverage_tests(coverage_map));
    }

    if test_map_is_empty(&test_map) {
        eprintln!("Nothing to do");
        return Ok(());
    }

    run::run_tests(&test_map, false)?;

    Ok(())
}

fn parse_line_specifications() -> Result<(PathLineMap, bool)> {
    let mut path_line_map = PathLineMap::default();
    let mut line_dash_used = false;
    for spec in &opts::get().lines {
        if spec == "-" {
            line_dash_used = true;
            continue;
        }
        let mut other = parse_line_specification(spec)?;
        path_line_map.append(&mut other);
    }
    Ok((path_line_map, line_dash_used))
}

fn read_diff() -> Result<PathLineMap> {
    let input = read_to_string(stdin())?;
    let mut patch_set = PatchSet::new();
    patch_set.parse(input)?;
    let mut path_line_map = PathLineMap::new();
    for patched_file in patch_set {
        if patched_file.source_file == "/dev/null" {
            continue;
        }
        let source_file = patched_file.source_file.strip_prefix("a/").ok_or_else(|| {
            anyhow!(
                r#"source file does not being with "a/": {}"#,
                patched_file.source_file
            )
        })?;
        let line_set = path_line_map.entry(source_file.to_owned()).or_default();
        for hunk in patched_file {
            // smoelius: Hmm. I'm not sure how best to handle insertions.
            if hunk.source_length == 0 {
                continue;
            }
            let start = u32::try_from(hunk.source_start)?;
            let end = u32::try_from(hunk.source_start + hunk.source_length)?;
            line_set.insert_range(start..end);
        }
    }
    Ok(path_line_map)
}

fn read_line_specifications() -> Result<PathLineMap> {
    BufReader::new(stdin())
        .lines()
        .try_fold(PathLineMap::new(), |mut path_line_map, result| {
            let line = result?;
            let mut other = parse_line_specification(&line)?;
            path_line_map.append(&mut other);
            Ok(path_line_map)
        })
}

#[allow(clippy::range_plus_one)]
fn parse_line_specification(spec: &str) -> Result<PathLineMap> {
    let (path, lines) = spec
        .rsplit_once(':')
        .ok_or_else(|| anyhow!("line specification does not contain `:`: {spec}"))?;
    let mut path_line_map = PathLineMap::default();
    let line_set = path_line_map.entry(path.to_owned()).or_default();
    for lines in lines.split(',') {
        let lines = if let Some((start, end)) = lines.split_once('-') {
            let start = start.parse::<u32>()?;
            let end = end.parse::<u32>()?;
            start..end + 1
        } else {
            let line = lines.parse::<u32>()?;
            line..line + 1
        };
        line_set.insert_range(lines);
    }
    Ok(path_line_map)
}

#[derive(Default)]
struct PathsNeedingWarning {
    nonexistent: Vec<String>,
    uncovered: Vec<String>,
}

fn validate_paths(db: &Db, path_line_map: &mut PathLineMap) -> Result<()> {
    let mut paths_needing_warning = PathsNeedingWarning::default();

    let mut result = Ok(());
    path_line_map.retain(|path, _| {
        if result.is_err() {
            return true;
        }
        #[allow(clippy::blocks_in_conditions)]
        match (|| -> Result<_> {
            if !Path::new(path).try_exists()? {
                paths_needing_warning.nonexistent.push(path.to_owned());
                return Ok(false);
            }
            if !db.path_digest_map.contains_key(path) {
                paths_needing_warning.uncovered.push(path.to_owned());
                return Ok(false);
            }
            Ok(true)
        })() {
            Ok(x) => x,
            Err(error) => {
                result = Err(error);
                true
            }
        }
    });
    let () = result?;

    warn_about_paths(paths_needing_warning)?;

    Ok(())
}

fn warn_about_paths(paths_needing_warning: PathsNeedingWarning) -> Result<()> {
    let PathsNeedingWarning {
        nonexistent,
        uncovered,
    } = paths_needing_warning;

    if !nonexistent.is_empty() {
        bail!("the following paths do not exist: {nonexistent:#?}",);
    }

    if !uncovered.is_empty() {
        warn(&format!(
            "the following paths are not covered by any test: {uncovered:#?}",
        ))?;
    }

    Ok(())
}

fn tests_for_path_lines(
    coverage_map: &PackageCrateMap<BTreeMap<Test, PathCoverageMap>>,
    path_line_map: &PathLineMap,
) -> Result<PackageCrateMap<Vec<Test>>> {
    let mut uncovered = path_line_map.clone();
    let mut test_map = PackageCrateMap::<Vec<Test>>::default();
    for (package, coverage_map) in coverage_map {
        let test_map = test_map.entry(package.clone()).or_default();
        for (krate, coverage_map) in coverage_map {
            let test_map = test_map.entry(krate.clone()).or_default();
            let mut added = false;
            for (test, coverage_map) in coverage_map {
                for (path, coverage) in coverage_map {
                    let Some(line_set) = path_line_map.get(path) else {
                        continue;
                    };
                    let uncovered = uncovered.get_mut(path).unwrap();
                    for &line in coverage {
                        if line_set.contains(line) && !added {
                            uncovered.remove(line);
                            test_map.push(test.clone());
                            added = true;
                        }
                    }
                }
            }
        }
    }

    warn_about_uncovered_lines(uncovered)?;

    Ok(test_map)
}

fn warn_about_uncovered_lines(path_line_map: PathLineMap) -> Result<()> {
    if path_line_map.values().all(RangeSet::is_empty) {
        return Ok(());
    }

    let mut msg = String::from("the following lines are not covered by any test:\n");

    for (path, line_set) in path_line_map {
        for Range { start, end } in line_set {
            let s = if start + 1 == end {
                start.to_string()
            } else {
                format!("{start}-{}", end - 1)
            };
            msg.push_str(&format!("    {path}:{s}\n"));
        }
    }

    warn(&msg)
}

fn zero_coverage_tests(
    coverage_map: PackageCrateMap<BTreeMap<Test, PathCoverageMap>>,
) -> PackageCrateMap<Vec<Test>> {
    let mut test_map = PackageCrateMap::<Vec<Test>>::default();
    for (package, coverage_map) in coverage_map {
        let test_map = test_map.entry(package.clone()).or_default();
        for (krate, coverage_map) in coverage_map {
            let test_map = test_map.entry(krate.clone()).or_default();
            for (test, coverage_map) in coverage_map {
                if coverage_map.values().map(HashSet::len).sum::<usize>() == 0 {
                    test_map.push(test);
                }
            }
        }
    }
    test_map
}

fn test_map_is_empty(test_map: &PackageCrateMap<Vec<Test>>) -> bool {
    test_map
        .values()
        .all(|test_map| test_map.values().all(Vec::is_empty))
}

fn refresh() -> Result<()> {
    let db = db::read()?;

    let coverage_map = db.coverage_map()?;

    let test_map = tests_for_refresh(&db, coverage_map)?;

    run::run_tests(&test_map, true)?;

    if !opts::get().no_run {
        db::build_digests()?;
    }

    Ok(())
}

fn tests_for_refresh(
    db: &Db,
    coverage_map: PackageCrateMap<BTreeMap<Test, PathCoverageMap>>,
) -> Result<PackageCrateMap<Vec<Test>>> {
    let mut test_map = PackageCrateMap::<Vec<Test>>::default();
    for (package, coverage_map) in coverage_map {
        let test_map = test_map.entry(package).or_default();
        for (krate, coverage_map) in coverage_map {
            let test_map = test_map.entry(krate).or_default();
            for (test, coverage_map) in coverage_map {
                for path in coverage_map.keys() {
                    if path_contents_changed(db, path)? {
                        test_map.push(test);
                        break;
                    }
                }
            }
        }
    }
    Ok(test_map)
}

fn path_contents_changed(db: &Db, path: &str) -> Result<bool> {
    let digest = hash_path_contents(path)?;
    Ok(db.path_digest_map.get(path) != Some(&digest))
}

#[cfg(test)]
mod test {
    use super::Opts;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        Opts::command().debug_assert();
    }
}
