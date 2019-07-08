#!/usr/bin/env bash

git submodule foreach "cargo clean && rm -f Cargo.lock"
cargo clean

