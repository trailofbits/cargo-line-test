use assert_cmd::cargo::cargo_bin_cmd;

#[test]
fn proc_macro() {
    let mut command = cargo_bin_cmd!(env!("CARGO_PKG_NAME"));
    command.args(["line-test", "--build"]);
    command.current_dir("fixtures/attr");
    command.assert().success();
}
