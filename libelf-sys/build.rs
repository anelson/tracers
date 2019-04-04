//! This builds the `elfutils` package from vendored source.  More specifically, it builds only
//! that subset of `elfutils` required to produce a statically linked `libelf.a`, compiled with
//! `-fPIC` to be compatible with Rust's linker.
//!
//! Many Linux distros have some kind of `libelf` package, but at least on Ubuntu 16.04 and
//! probably many others, the static lib is not compiled with `-fPIC` and thus can't be used.
//! Rather than require the user of the `probe-rs` crate to deal with this, it's easier to just
//! build directly from source.
extern crate cc;

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // The build for elfutils is pretty standard autotools and GNU make, with some mild trickery to
    // get an -fPIC compiled static lib
    let dst = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let root = dst.join("libelf");
    let include = root.join("include");
    let build = root.join("build");
    let lib = root.join("lib");
    fs::create_dir_all(&build).unwrap();
    fs::create_dir_all(&include).unwrap();
    fs::create_dir_all(&lib).unwrap();

    let src_path = fs::canonicalize(Path::new("vendor/libelf")).unwrap();

    //Based on a technique used in `curl-rust`'s `build.rs`, invoke the `./configure` script with
    //`sh`, using the `cc` crate to get the build-related environment variables
    let cfg = cc::Build::new();
    let compiler = cfg.get_compiler();
    let mut cflags = OsString::new();
    for arg in compiler.args() {
        cflags.push(arg);
        cflags.push(" ");
    }

    let mut config = Command::new("sh");
    config
        .env("CC", compiler.path())
        .env("CFLAGS", cflags)
        .env("VERBOSE", "1")
        .current_dir(&build)
        .arg(&src_path.join("configure"))
        .arg(&format!("--prefix={}", root.display()));
    run(&mut config, "configure");

    //now make the elfutils.  TODO: Someday see if there's a reliable way to only build `libelf`
    //and its dependencies
    let mut make = Command::new("make");
    make.current_dir(&build);
    make.arg("install");
    run(&mut make, "make install");

    //because we ran `make install`, libs and headers were copied to the `root` directory.  For
    //reasons I don't understand, the makefiles build a static version of `libelf` with `-fPIC`,
    //called `libelf_pic.a`, but this file is specifically NOT copied to the install directory.
    //Thus, let's copy it ourselves
    let libelf_static_pic_path = build.join("libelf/libelf_pic.a");
    let libelf_output_path = lib.join("libelf.a");
    fs::copy(libelf_static_pic_path.clone(), libelf_output_path).expect(&format!(
        "failed to copy library file {:?}",
        libelf_static_pic_path
    ));

    //Tell cargo to link with the static elf library and point to where to find it and the header
    //files
    println!("cargo:root={}", root.display());
    println!("cargo:include={}", include.display());
    println!("cargo:rustc-link-lib=static={}", "elf");
    println!("cargo:rustc-link-search=native={}", lib.display());
}

fn run(cmd: &mut Command, program: &str) {
    println!("running: {:?}", cmd);
    let status = match cmd.status() {
        Ok(status) => status,
        Err(ref e) if e.kind() == ErrorKind::NotFound => {
            fail(&format!(
                "failed to execute command: {}\nis `{}` not installed?",
                e, program
            ));
        }
        Err(e) => fail(&format!("failed to execute command: {}", e)),
    };
    if !status.success() {
        fail(&format!(
            "command did not execute successfully, got: {}",
            status
        ));
    }
}

fn fail(s: &str) -> ! {
    panic!("\n{}\n\nbuild script failed, must exit now", s)
}
