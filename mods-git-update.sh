#!/usr/bin/env bash

git submodule update --remote --recursive
git submodule foreach git checkout master
git submodule foreach git pull
