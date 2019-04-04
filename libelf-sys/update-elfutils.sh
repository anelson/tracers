#!/usr/bin/env sh
#
# The elfutils code in the git repo is not ready to build from source
# It's missing the `configure` script and requires some machinations that I have no patience for.  So instead
# we'll use the latest source tarball.
#
# This script is for maintainers to use to pull in the latest elfutils code.
# The intention is to check the code in to source control because it's bad form for cargo builds
# to go out on the internet and do stuff
ELFUTILS_URL="https://sourceware.org/elfutils/ftp/elfutils-latest.tar.bz2"
VENDOR_DIR="vendor/libelf"

mkdir -p $VENDOR_DIR
curl --silent $ELFUTILS_URL | tar -xvj --directory=$VENDOR_DIR --recursive-unlink --strip-components=1
