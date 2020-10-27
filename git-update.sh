#!/usr/bin/env bash

git pull
git submodule update --init --remote
git submodule foreach "git checkout master && git pull"
