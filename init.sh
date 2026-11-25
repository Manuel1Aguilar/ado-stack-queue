#!/bin/sh
# Session start: build and run every test command; stop at the first failure.
set -e
cd "$(dirname "$0")"
cargo test --workspace --locked
