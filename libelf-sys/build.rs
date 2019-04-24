//! This builds the `elfutils` package from vendored source.  More specifically, it builds only
//! that subset of `elfutils` required to produce a statically linked `libelf.a`, compiled with
//! `-fPIC` to be compatible with Rust's linker.
//!
//! Many Linux distros have some kind of `libelf` package, but at least on Ubuntu 16.04 and
//! probably many others, the static lib is not compiled with `-fPIC` and thus can't be used.
//! Rather than require the user of the `tracers` crate to deal with this, it's easier to just
//! build directly from source.
extern crate cc;

use failure::{bail, format_err, Fallible};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

fn is_enabled() -> bool {
    env::var("CARGO_FEATURE_ENABLED").is_ok() || is_required()
}

fn is_required() -> bool {
    env::var("CARGO_FEATURE_REQUIRED").is_ok()
}

fn main() {
    //by default we don't do anything here unless this lib is explicitly enabled
    if !is_enabled() {
        println!("libelf-sys is not enabled; build skipped");
        return;
    }

    let fail_on_error = is_required();

    match try_build() {
        Ok(_) => {
            //Build succeeded, which means the Rust bindings should be enabled and
            //dependent crates should be signaled that this lib is available
            println!("cargo:rustc-cfg=enabled");
            println!("cargo:succeeded=1"); //this will set DEP_ELF_SUCCEEDED in dependent builds
        }
        Err(e) => {
            if fail_on_error {
                panic!("libelf-sys build failed: {}", e);
            } else {
                println!("cargo:WARNING=libelf-sys build failed: {}", e);
                println!("cargo:WARNING=the libelf-sys bindings will not be included in the crate");
            }
        }
    }
}

fn try_build() -> Fallible<()> {
    //Though undocumented, cargo exposes all the info we need about the target
    //see https://kazlauskas.me/entries/writing-proper-buildrs-scripts.html

    if env::var("CARGO_CFG_TARGET_OS")? != "linux" {
        bail!("libelf-sys is only supported on Linux")
    } else if env::var("CARGO_CFG_TARGET_ARCH")? != "x86_64" {
        bail!("libelf-sys is only supported on x86_64 architectures")
    }

    // The build for elfutils is pretty standard autotools and GNU make, with some mild trickery to
    // get an -fPIC compiled static lib
    let dst = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let root = dst.join("libelf");
    let include = root.join("include");
    let build = root.join("build");
    let lib = root.join("lib");
    fs::create_dir_all(&build)?;
    fs::create_dir_all(&include)?;
    fs::create_dir_all(&lib)?;

    let src_path = fs::canonicalize(Path::new("vendor/libelf"))?;

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
    run(&mut config, "configure")?;

    //now make the elfutils.  TODO: Someday see if there's a reliable way to only build `libelf`
    //and its dependencies
    let mut make = Command::new("make");
    make.current_dir(&build);
    make.arg("install");
    run(&mut make, "make install")?;

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

    Ok(())
}

fn run(cmd: &mut Command, program: &str) -> Fallible<()> {
    println!("running: {:?}", cmd);
    match cmd.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format_err!(
            "'{}' failed with status code {}",
            program,
            status
        )),
        Err(ref e) if e.kind() == ErrorKind::NotFound => Err(format_err!(
            "failed to execute command [{}] because the command was not found",
            program
        )),
        Err(e) => Err(format_err!("failed to execute command: {}", e)),
    }
}
