#!/usr/bin/env bash
set -e
cd op-cli
cargo test --features system-tests -- --test-threads=1
