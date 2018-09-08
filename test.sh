#!/bin/bash

set -euxo pipefail

rm -rf ./test/output
mkdir -p ./test/output

cp ./test/template/Cargo.toml ./test/output/Cargo.toml
# cargo run -- -i ./test/resources/stm-lib.rs -o ./test/output/src
cargo build
RUST_LOG="trace" ./target/debug/form -i ./test/resources/small-lib.rs -o ./test/output

# cd ./test/output
# cargo fmt
# cargo check --target thumbv7em-none-eabihf

# cd ../../
