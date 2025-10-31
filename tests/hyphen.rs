use assert_cmd::cargo::cargo_bin_cmd;

// smoelius: https://github.com/trailofbits/cargo-line-test/issues/36
#[test]
fn library_name_with_hyphen() {
    let mut command = cargo_bin_cmd!(env!("CARGO_PKG_NAME"));
    command.args(["line-test", "--build"]);
    command.current_dir("fixtures/my-package");
    command.assert().success();
}
