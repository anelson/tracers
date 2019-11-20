#!/usr/bin/env bash
#
# Manually re-generate the Rust bindings.  This isn't normally required
# but if there's a change in the library ABI then re-generate the bindings
#
# First make sure bindgen is installed with `cargo install bindgen`, and then
# run this script.  This uses the vendored libstapsdt header files so make sure
# to  run the git submodule update command.
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"

header_file="$SCRIPT_DIR/src/lib.h"
include_path="$SCRIPT_DIR/vendor/libelf/libelf"
rust_file="$SCRIPT_DIR/src/libelf.rs"

bindgen --no-layout-tests \
    --output "$rust_file" \
    "$header_file" \
    -- "-I${include_path}"

