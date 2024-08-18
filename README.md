# cargo-line-test

Run tests by the lines they exercise

`cargo-line-test` relies on [`cargo-llvm-cov`] and requires it to be installed independently:

```sh
cargo install cargo-line-test cargo-llvm-cov
```

## Examples

```sh
# Build cargo-line-test database
cargo line-test --build

# Run the tests that exercise src/main.rs:99
cargo line-test --line src/main.rs:99

# Run the tests that exercise lines changed by diff
git diff | cargo line-test --diff

# Update the database following source code changes
cargo line-test --refresh
```

[`cargo-llvm-cov`]: https://crates.io/crates/cargo-llvm-cov
