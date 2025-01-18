use assert_cmd::cargo::CommandCargoExt;
use std::process::Command;

// smoelius: https://github.com/trailofbits/cargo-line-test/issues/36
#[test]
fn library_name_with_hyphen() {
    let mut command = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
    command.args(["line-test", "--build"]);
    command.current_dir("fixtures/my-package");
    let status = command.status().unwrap();
    assert!(status.success());
}
