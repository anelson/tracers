extern crate cc;
extern crate pkg_config;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    if env::var("CARGO_CFG_TARGET_ARCH").unwrap() != "x86_64" {
        panic!("libstapsdt is only supported on 64-bit Intel x86");
    }

    if env::var("CARGO_CFG_TARGET_OS").unwrap() != "linux" {
        panic!("libstapsdt is only supported on Linux");
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
    let dst = fs::canonicalize(PathBuf::from(env::var_os("OUT_DIR").unwrap())).unwrap();
    let root = dst.join("libstapsdt");
    let build = root.join("build");
    let lib = root.join("lib");
    let include = src_path.clone(); //libstapsdt doesn't segregate include from src files

    //the libstapstd Makefile is not idempotent, it will actually fail if `make install` is run a
    //second time.  So, ensure we always build with a clean slate
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&build).unwrap();
    fs::create_dir_all(&lib).unwrap();

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
    cfg.include(&env::var("DEP_ELF_INCLUDE").expect("the libelf-sys crate should provide this"));
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

    run(&mut make, "make");

    //The makefile doesn't copy the static lib anywhere so do that ourselves
    let libstapsdt_static_path = src_path.join("out/libstapsdt.a");
    let libstapsdt_output_path = lib.join("libstapsdt.a");
    fs::copy(libstapsdt_static_path.clone(), libstapsdt_output_path).expect(&format!(
        "failed to copy library file {:?}",
        libstapsdt_static_path
    ));

    // we must explicitly tell cargo that it should statically link with the `elf` lib which the
    // `libelf-sys` crate already ensures is compiled and in the library search  path
    println!("cargo:rustc-link-lib=static={}", "elf");
    println!("cargo:rustc-link-lib=static={}", "stapsdt");
    println!("cargo:rustc-link-search=native={}", lib.display());
    println!("cargo:include={}", include.display());
    println!("cargo:root={}", root.display());
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
