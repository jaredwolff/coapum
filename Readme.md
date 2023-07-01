## Code coverage:

This requires you have `grcov` installed.

```
$ cargo install grcov
```

Then run coverage reports:

```
$ CARGO_INCREMENTAL=0 RUSTFLAGS='-Cinstrument-coverage' LLVM_PROFILE_FILE='cargo-test-%p-%m.profraw' cargo test
$ grcov . --binary-path ./target/debug/ -s . -t html --branch --ignore-not-existing --ignore "target/*" --ignore "src/bin/*" -o target/coverage/
$ grcov . --binary-path ./target/debug/ -s . -t lcov --branch --ignore-not-existing --ignore "target/*" --ignore "src/bin/*" -o target/coverage/tests.lcov
```