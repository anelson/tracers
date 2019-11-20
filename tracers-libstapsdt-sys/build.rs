extern crate cc;
extern crate pkg_config;

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
        println!("libstapsdt-sys is not enabled; build skipped");
        return;
    }

    let fail_on_error = is_required();

    match try_build() {
        Ok(_) => {
            //Build succeeded, which means the Rust bindings should be enabled and
            //dependent crates should be signaled that this lib is available
            println!("cargo:rustc-cfg=enabled");
            println!("cargo:succeeded=1"); //this will set DEP_(PKGNAME)_SUCCEEDED in dependent builds
        }
        Err(e) => {
            if fail_on_error {
                panic!("libstapsdt-sys build failed: {}", e);
            } else {
                println!("cargo:warning=libstapsdt-sys build failed: {}", e);
                println!(
                    "cargo:warning=the libstapsdt-sys bindings will not be included in the crate"
                );
            }
        }
    }
}

fn try_build() -> Fallible<()> {
    if env::var("CARGO_CFG_TARGET_OS")? != "linux" {
        bail!("libstapsdt-sys is only supported on Linux")
    } else if env::var("CARGO_CFG_TARGET_ARCH")? != "x86_64" {
        bail!("libstapsdt-sys is only supported on x86_64 architectures")
    }

    if env::var("DEP_ELF_SUCCEEDED").is_err() {
        bail!("libstapsdt-sys is not available because libelf-sys did not build successfully")
    }

    // Init the submodule if not already where the source code is located
    let src_path = fs::canonicalize(Path::new("vendor/libstapsdt")).unwrap();
    if !src_path.join("/.git").exists() {
        let _ = Command::new("git")
            .args(&["submodule", "update", "--init"])
            .status();
    }

    //The makefile for libstapsdt is mercifully simple, and since it's wrapping a Linux-only
    //subsystem SytemTap there's no cross-platform nonsense either.
    let dst = fs::canonicalize(PathBuf::from(env::var("OUT_DIR")?))?;
    let root = dst.join("libstapsdt");
    let build = root.join("build");
    let lib = root.join("lib");
    let include = src_path.clone(); //libstapsdt doesn't segregate include from src files

    //the libstapstd Makefile is not idempotent, it will actually fail if `make install` is run a
    //second time.  So, ensure we always build with a clean slate
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&build)?;
    fs::create_dir_all(&lib)?;

    //We won't use cc to build the library, it has a makefile
    //But the cc crate will help us build up the right CFLAGS
    let mut cfg = cc::Build::new();
    cfg.warnings(true)
        .warnings_into_errors(false) //it pains me to do this but the lib doesn't compile clean
        .flag("-z")
        .flag("noexecstack")
        .pic(true)
        .out_dir(&build);

    // The libelf-sys crate which wraps the libelf library will provide the path to its include
    // files.  The lib itself will already be linked via cargo magic
    cfg.include(&env::var("DEP_ELF_INCLUDE")?);
    let compiler = cfg.get_compiler();
    let mut cflags = OsString::from("-std=gnu11 ");
    for arg in compiler.args() {
        cflags.push(arg);
        cflags.push(" ");
    }

    //NB: I am deliberately overriding the CC and CFLAGS variabes hard-coded in the `Makefile` with
    //values computed using the `cc` crate.  As of this writing, that works.  It's possible a
    //future change to `libstapsdt-sys` might add more flags that will then not be reflected here
    //and break the build.  If so that's the first place to look.
    let mut make = Command::new("make");
    make.arg("--environment-overrides")
        .arg("out/libstapsdt.a")
        .env("CC", compiler.path())
        .env("CFLAGS", cflags)
        .env("PREFIX", root.clone())
        .env("VERBOSE", "1")
        .current_dir(&src_path);

    run(&mut make, "make")?;

    //The makefile doesn't copy the static lib anywhere so do that ourselves
    let libstapsdt_static_path = src_path.join("out/libstapsdt.a");
    let libstapsdt_output_path = lib.join("libstapsdt.a");
    fs::copy(libstapsdt_static_path.clone(), libstapsdt_output_path)?;

    // we must explicitly tell cargo that it should statically link with the `elf` lib which the
    // `libstapsdt-sys` crate already ensures is compiled and in the library search  path
    println!("cargo:rustc-link-lib=static={}", "elf");
    println!("cargo:rustc-link-lib=static={}", "stapsdt");
    println!("cargo:rustc-link-search=native={}", lib.display());
    println!("cargo:include={}", include.display());
    println!("cargo:root={}", root.display());

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
