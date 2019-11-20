#!/usr/bin/env bash
#
# Does a dry run on publishing all crates to check for problems
set -e
SCRIPTPATH="$( cd "$(dirname "$0")" ; pwd -P )"

CRATES=( \
    tracers-core \
    tracers-libelf-sys \
    tracers-libstapsdt-sys \
    tracers-codegen \
    tracers-build \
    tracers-macros-hack \
    tracers-macros  \
    tracers-dyn-noop \
    tracers-dyn-stap \
    tracers \
    )

echo "Running cargo publish --dry-run on all crates"

for crate in "${CRATES[@]}"
do
    echo "Pre-publishing crate $crate"
    cargo publish --dry-run --manifest-path "$SCRIPTPATH/../$crate/Cargo.toml"
done
