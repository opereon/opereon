#!/usr/bin/env bash
set -e
cd op-cli
cargo test --features integration-tests -- --test-threads=1
