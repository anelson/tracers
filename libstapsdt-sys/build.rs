extern crate cc;
extern crate pkg_config;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use glob::glob;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    if env::var("CARGO_CFG_TARGET_ARCH").unwrap() != "x86_64" {
        panic!("libstapsdt is only supported on 64-bit Intel x86");
    }

    if env::var("CARGO_CFG_TARGET_OS").unwrap() != "linux" {
        panic!("libstapsdt is only supported on Linux");
    }

    // By default this statically links to libstapsdt.  That can be overriden
    let wants_dynamic = env::var("LIBSTAPSDT_DYNAMIC").is_ok();
    let statik = if wants_dynamic { "" } else { "static=" };
    let libext = if wants_dynamic { "so" } else { "a" };

    // It's unlikely pkg_config knows about this, since the library's own deb package doesn't
    // register the library with pkg-config.  However it doesn't hurt to try.
    if let Ok(pkg) = pkg_config::Config::new()
        .statik(!wants_dynamic)
        .probe("libstapsdt")
    {
        for path in &pkg.include_paths {
            println!("cargo:include={}", path.display());
        }

        for path in &pkg.link_paths {
            println!("cargo:rustc-link-search=native={}", path.display());
        }

        for lib in &pkg.libs {
            println!("cargo:rustc-link-lib={}{}", statik, lib);
        }

        return;
    }

    // no matter what, tell cargo to link with this libstapd library, either the one that's
    // installed or the one we'll build below
    println!("cargo:rustc-link-lib={}{}", statik, "stapsdt");

    //The makefile for libstapsdt is mercifully simple, and since it's wrapping a Linux-only
    //subsystem SytemTap there's no cross-platform nonsense either.
    let dst = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let include = dst.join("include");
    let build = dst.join("build");
    println!("cargo:include={}", include.display());
    println!("cargo:root={}", dst.display());
    fs::create_dir_all(&build).unwrap();
    fs::create_dir_all(&include).unwrap();

    let mut cfg = cc::Build::new();
    cfg.warnings(true)
        .warnings_into_errors(true)
        .static_flag(!wants_dynamic)
        .flag_if_supported("-std=gnu11")
        .flag_if_supported("-Wl")
        .flag("-lelf")
        .flag("-ldl")
        .flag("-z")
        .flag("noexecstack")
        .pic(true)
        .out_dir(&build);

    // Accept overrides for include and lib paths
    if let Ok(include_dir) = env::var("LIBSTAPSDT_INCLUDE_DIR") {
        println!("cargo:include={}", include_dir);
        cfg.include(include_dir);
    }

    if let Ok(lib_dir) = env::var("LIBSTAPSDT_LIB_DIR") {
        println!("cargo:rustc-link-search=native={}", lib_dir);
        cfg.flag(&format!("-L{}", lib_dir));
    }

    // It's also possible that libstapsdt has already been installed on the system via the package
    // or building it explicitly from source
    if libstapsdt_installed(&cfg) {
        return;
    }

    // We're reduced to building from source.  Init the submodule if not already
    let src_path = Path::new("vendor/libstapsdt");
    if !src_path.join("/.git").exists() {
        let _ = Command::new("git")
            .args(&["submodule", "update", "--init"])
            .status();
    }

    //Copy the header files to the output directory in case downstream crates need to use them
    for header in glob(src_path.join("src/*.h").to_str().unwrap())
        .unwrap()
        .map(|x| x.unwrap())
    {
        fs::copy(header.clone(), include.join(header.file_name().unwrap()))
            .expect(&format!("failed to copy header file {:?}", header));
    }

    cfg.files(
        glob(src_path.join("src/*.h").to_str().unwrap())
            .unwrap()
            .map(|x| x.unwrap()),
    )
    .file(src_path.join("src/asm/libstapsdt-x86_64.s"))
    .compile(&format!("libstapsdt.{}", libext));
}

fn libstapsdt_installed(cfg: &cc::Build) -> bool {
    //Try to build an executable that links to the static library using the
    //systemwide defaults.

    //Start with a copy of the cc config, but cc only supports building libraries
    //so at some point we'll have to override it
    let mut cfg = cfg.clone();
    cfg.file("src/test/libstapsdt.c").flag("-lstapsdt");

    let compiler = cfg.get_compiler();
    let mut cmd = compiler.to_command();
    cmd.arg("-o").arg("/dev/null").arg("-lstapsdt");

    println!("running {:?}", cmd);
    if let Ok(status) = cmd.status() {
        if status.success() {
            return true;
        }
    }

    false
}
