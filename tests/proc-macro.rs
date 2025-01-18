use assert_cmd::cargo::CommandCargoExt;
use std::process::Command;

#[test]
fn proc_macro() {
    let mut command = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
    command.args(["line-test", "--build"]);
    command.current_dir("fixtures/attr");
    let status = command.status().unwrap();
    assert!(status.success());
}
