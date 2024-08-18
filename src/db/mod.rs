use crate::{PackageCrateMap, PathCoverageMap, PathDigestMap, Test};
use anyhow::Result;
use std::collections::BTreeMap;

mod build;
mod read;

pub struct Db {
    pub package_crate_test_map: PackageCrateMap<Vec<Test>>,
    pub path_digest_map: PathDigestMap,
}

impl Db {
    pub fn coverage_map(&self) -> Result<PackageCrateMap<BTreeMap<Test, PathCoverageMap>>> {
        read::read_coverage_map(&self.package_crate_test_map)
    }
}

pub fn build() -> Result<()> {
    build::build()
}

pub fn build_digests() -> Result<()> {
    build::build_digests()
}

pub fn read() -> Result<Db> {
    read::read()
}
