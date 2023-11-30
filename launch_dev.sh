#!/usr/bin/env bash

cd "$(dirname "${BASH_SOURCE[0]}")"
export RUST_LOG=debug
export SCANNER_PATH="$(pwd)/target/release/scanner"
cargo build --release --bin scanner
cargo run $@ --bin sn-tracer-egui