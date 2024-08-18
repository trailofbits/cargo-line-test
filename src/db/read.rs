use super::{Db, PathDigestMap};
use crate::{CrateMap, PackageCrateMap, PathCoverageMap, Test};
use anyhow::{anyhow, bail, ensure, Result};
use lcov::{Reader, Record};
use std::{
    collections::{BTreeMap, HashSet},
    env::current_dir,
    ffi::OsStr,
    fs::{read_dir, read_to_string},
    os::unix::ffi::OsStrExt,
    path::Path,
};

pub(super) fn read() -> Result<Db> {
    let package_crate_test_map = read_package_crate_test_map()?;
    let path_digest_map = read_path_digest_map()?;

    Ok(Db {
        package_crate_test_map,
        path_digest_map,
    })
}

pub(super) fn read_package_crate_test_map() -> Result<PackageCrateMap<Vec<Test>>> {
    let mut package_crate_test_map = PackageCrateMap::<Vec<Test>>::default();
    for result in read_dir("line-test.db/packages")? {
        let entry = result?;
        let path = entry.path();
        let file_stem = path.file_stem_utf8(None)?;
        let crate_map = read_package_dir(&path)?;
        package_crate_test_map.insert(file_stem.to_owned(), crate_map);
    }
    Ok(package_crate_test_map)
}

fn read_package_dir(path: &Path) -> Result<CrateMap<Vec<Test>>> {
    let mut crate_test_map = CrateMap::<Vec<Test>>::default();
    for result in read_dir(path)? {
        let entry = result?;
        let path = entry.path();
        let file_stem = path.file_stem_utf8(None)?;
        let tests = read_crate_dir(&path)?;
        crate_test_map.insert(file_stem.to_owned(), tests);
    }
    Ok(crate_test_map)
}

fn read_crate_dir(path: &Path) -> Result<Vec<Test>> {
    let mut tests = Vec::<Test>::default();
    for result in read_dir(path)? {
        let entry = result?;
        let path = entry.path();
        let file_stem = path.file_stem_utf8(Some("lcov"))?;
        tests.push(file_stem.split("::").map(ToOwned::to_owned).collect());
    }
    Ok(tests)
}

fn read_path_digest_map() -> Result<PathDigestMap> {
    let json = read_to_string("line-test.db/digests.json")?;
    let path_hex_map = serde_json::from_str::<BTreeMap<String, String>>(&json)?;
    let mut path_digest_map = BTreeMap::new();
    for (path, hex) in path_hex_map {
        let digest_vec = hex::decode(&hex)?;
        let digest =
            <[u8; 32]>::try_from(digest_vec).map_err(|_| anyhow!("invalid digest: {hex}"))?;
        path_digest_map.insert(path, digest);
    }
    Ok(path_digest_map)
}

pub(super) fn read_coverage_map(
    package_crate_test_map: &PackageCrateMap<Vec<Test>>,
) -> Result<PackageCrateMap<BTreeMap<Test, PathCoverageMap>>> {
    let mut coverage_map = PackageCrateMap::<BTreeMap<Test, PathCoverageMap>>::default();
    for (package, crate_test_map) in package_crate_test_map {
        let coverage_map = coverage_map.entry(package.clone()).or_default();
        for (krate, tests) in crate_test_map {
            let coverage_map = coverage_map.entry(krate.clone()).or_default();
            for test in tests {
                let path_buf = Path::new("line-test.db/packages")
                    .join(package)
                    .join(krate)
                    .join(test.to_string())
                    .with_extension("lcov");
                let path_coverage_map = read_lcov(&path_buf)?;
                coverage_map.insert(test.clone(), path_coverage_map);
            }
        }
    }
    Ok(coverage_map)
}

fn read_lcov(path: &Path) -> Result<PathCoverageMap> {
    let current_dir = current_dir()?;
    let mut path_coverage_map = PathCoverageMap::default();
    let mut source_file = None;
    let mut coverage = HashSet::new();
    for result in Reader::open_file(path)? {
        match result? {
            Record::SourceFile { path } => {
                if let Some(source_file) = source_file {
                    bail!("source file already given: {source_file}");
                }
                let path = path.strip_prefix(&current_dir)?;
                let path_utf8 = std::str::from_utf8(path.as_os_str().as_bytes())?;
                source_file = Some(path_utf8.to_owned());
            }
            Record::LineData {
                line,
                count,
                checksum: _,
            } if count != 0 => {
                coverage.insert(line);
            }
            Record::EndOfRecord => {
                let Some(key) = source_file else {
                    bail!("source file not given");
                };
                path_coverage_map.insert(key, coverage);
                source_file = None;
                coverage = HashSet::new();
            }
            _ => {}
        }
    }
    Ok(path_coverage_map)
}

trait FileStemUtf8 {
    fn file_stem_utf8(&self, expected_extension: Option<&str>) -> Result<&str>;
}

impl FileStemUtf8 for Path {
    fn file_stem_utf8(&self, expected_extension: Option<&str>) -> Result<&str> {
        let extension = self.extension();
        ensure!(
            expected_extension.map(OsStr::new) == extension,
            "unexpected file extension: {extension:?}"
        );
        let Some(file_stem_os) = self.file_stem() else {
            bail!("path has no file stem: {}", self.display());
        };
        std::str::from_utf8(file_stem_os.as_bytes()).map_err(Into::into)
    }
}
