#!/bin/sh
set -eu
cd "$(dirname "$0")"
cargo build --release --locked
cargo test --locked
printf '\nBuilt: %s/target/release/prychal-dock\n' "$PWD"
