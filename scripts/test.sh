#!/usr/bin/env bash
#
# Builds the project on UNIX-like systems
set -u
set -e

cargo test \
    --package probers \
    --package probers-macros \
    "$@"

