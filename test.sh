#!/bin/bash

set -euxo pipefail

rm -rf ./test/output
mkdir -p ./test/output/src

cp ./test/template/Cargo.toml ./test/output/Cargo.toml
cargo run -- -i ./test/resources/stm-lib.rs -o ./test/output/src

cd ./test/output
cargo fmt
cargo check --target thumbv7em-none-eabihf

cd ../../
