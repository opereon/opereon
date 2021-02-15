#!/usr/bin/env bash

git submodule foreach cargo upgrade --workspace
cargo upgrade --workspace
