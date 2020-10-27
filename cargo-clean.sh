#!/usr/bin/env bash

git submodule foreach "cargo clean && find . -name Cargo.lock -type f -delete"
cargo clean

