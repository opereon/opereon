#!/usr/bin/env bash
set -e
cd op-cli
RUST_BACKTRACE=1 cargo test --features system-tests -- --test-threads=1
