//! This module is responsible for generating the native C wrappers for each provider, which thunk
//! calls to the target platform's native tracing mechanism.  Most of the code generation logic is
//! the same across all platforms, with platform specific bits factored out into one of the
//! `platform` submodules
use crate::build_rs::BuildInfo;
use crate::cache;
use crate::deps::{self, SourceDependency};
use crate::spec::{self, ProviderSpecification};
use crate::TracersResult;
use crate::TracingTarget;
use crate::TracingType;
use failure::ResultExt;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

mod target;

/// The (possibly cached) data structure containing the results of processing a Rust source file
#[derive(Serialize, Deserialize)]
struct ProcessedFile {
    dependencies: Vec<SourceDependency>,
    providers: Vec<ProviderSpecification>,
}

/// The (possibly cached) data structure containing the results of running code gen on a provider
/// trait
#[derive(Serialize, Deserialize)]
struct ProcessedProviderTrait {
    lib_path: PathBuf,
    bindings_path: PathBuf,
}

trait NativeCodeGenerator {
    /// Generates a native static library that wraps the platform-speciifc probing calls in
    /// something that Rust's FFI can handle
    fn generate_native_lib(&self) -> TracersResult<PathBuf>;

    /// Generates Rust bindings which wrap the native lib in somethign Rust-callable
    fn generate_rust_bindings(&self, native_lib_path: &Path) -> TracersResult<PathBuf>;

    fn out_dir(&self) -> &Path;

    fn build_dir(&self) -> PathBuf {
        self.out_dir().join("build")
    }

    fn output_dir(&self) -> PathBuf {
        self.out_dir().join("output")
    }
}

pub(super) fn generate_native_code(
    build_info: &BuildInfo,
    stdout: &mut dyn Write,
    manifest_dir: &Path,
    out_dir: &Path,
    _package_name: &str,
    targets: Vec<PathBuf>,
) {
    assert!(build_info.implementation.tracing_type() == TracingType::Static);

    match build_info.implementation.tracing_target() {
        TracingTarget::Disabled | TracingTarget::NoOp => {
            writeln!(
                stdout,
                "No native code needed for {} tracing",
                build_info.implementation.tracing_target().as_ref()
            )
            .unwrap();
        }
        TracingTarget::Stap => {
            for target in targets.into_iter() {
                let target_path = manifest_dir.join(&target);
                writeln!(stdout, "Processing target {}", target_path.display()).unwrap();
                process_file(build_info, stdout, out_dir, &target_path);
            }
        }
    };
}

fn process_file(build_info: &BuildInfo, stdout: &mut dyn Write, out_dir: &Path, file: &Path) {
    //Find the dependent files and providers in this source file, retrieving that info from cache
    //if we've done this before
    let cache_dir = cache_dir(out_dir);
    let result =
        cache::cache_file_computation(&cache_dir, file, "processed-file", |file_contents| {
            writeln!(
                stdout,
                "Generating {} implementation for target {}",
                build_info.implementation.tracing_target().as_ref(),
                file.display()
            )
            .unwrap();

            //Parse this Rust source file
            let file: syn::File = syn::parse_file(file_contents).context("Parsing source file")?;

            //Scan the AST for additional modules in external source files
            //We're not processing these yet, but we will return the list of dependencies so that it is
            //cached along with the providers in this file.
            let dependencies = deps::get_dependencies(&file);

            //Scan the AST for provider traits
            let providers = spec::find_providers(&file);

            Ok(ProcessedFile {
                dependencies,
                providers,
            })
        });

    match result {
        Ok(processed_file) => {
            //Maybe cached maybe not, we got the info for this file
            //Generate code for the providers, and recursively process all dependent files
            for dependency in processed_file.dependencies.into_iter() {
                match deps::resolve_dependency(file, &dependency) {
                    // Dependency resolved; recursively process this one also
                    Ok(dep_file) => process_file(build_info, stdout, out_dir, &dep_file),

                    // Failed to resolve dependency.  This code probably won't compile anyway, but log
                    // a warning through Cargo so the user understands the generation step wasn't
                    // successful either
                    Err(_) => {
                        writeln!(stdout,
                             "cargo:WARNING=Unable to resove dependency {:?} in {}; any tracing providers it may contain will not be processed",
                             dependency,
                             file.display()
                             ).unwrap();
                    }
                }
            }

            for provider in processed_file.providers.into_iter() {
                //Call `process_provider` for each provider in the file.  If it fails, log the failure
                //in a way that will cause Cargo to report a warning, and continue on
                process_provider(build_info, stdout, out_dir, file, provider);
            }
        }
        Err(e) => {
            //Failures to process a single file should not fail this call.  The proc macros
            //will handle reporting any errors
            writeln!(
                stdout,
                "cargo:WARNING=Error processing '{}': {}",
                file.display(),
                e
            )
            .unwrap();
            writeln!(
                stdout,
                "cargo:WARNING=Code generation failed for '{}'.  Tracing may not be available.",
                file.display()
            )
            .unwrap();
        }
    };
}

fn process_provider(
    build_info: &BuildInfo,
    stdout: &mut dyn Write,
    out_dir: &Path,
    _file: &Path,
    provider: ProviderSpecification,
) {
    let cache_dir = cache_dir(out_dir);

    // For this trait, generate native and rust code for it.  If this trait was processed before
    // and hasn't changed, even if the source file it's in has changed, then we can skip that
    // generation and used the cached result
    let token_stream = provider.token_stream().clone();
    let name = provider.name().to_owned();
    let ident = provider.ident().clone();
    let result =
        cache::cache_tokenstream_computation(&cache_dir, &token_stream, &name, move |_| {
            let generator = create_native_code_generator(build_info, out_dir, provider);

            let lib_path = generator.generate_native_lib()?;
            let bindings_path = generator.generate_rust_bindings(&lib_path)?;

            Ok(ProcessedProviderTrait {
                lib_path,
                bindings_path,
            })
        });

    match result {
        Ok(processed_provider) => {
            //Output commands to cargo to ensure it can locate this static library.  The Rust
            //bindings will be injected by the `tracer` proc macro
            let lib_directory = processed_provider
                .lib_path
                .parent()
                .expect("lib must have a parent");
            let lib_filename = processed_provider
                .lib_path
                .file_stem()
                .expect("lib must have a file name");

            writeln!(
                stdout,
                "cargo:rustc-link-lib=static={}",
                lib_filename.to_str().expect("filename isn't valid")
            )
            .unwrap();
            writeln!(
                stdout,
                "cargo:rustc-link-search=native={}",
                lib_directory.display()
            )
            .unwrap();
        }
        Err(e) => {
            writeln!(
                stdout,
                "cargo:WARNING=Error generating tracing code for '{}': {}",
                ident, e
            )
            .unwrap();
            writeln!(
                stdout,
                "cargo:WARNING=Tracing may not be available for {}",
                ident
            )
            .unwrap();
        }
    }
}

fn create_native_code_generator(
    build_info: &BuildInfo,
    out_dir: &Path,
    provider: ProviderSpecification,
) -> Box<NativeCodeGenerator> {
    match build_info.implementation.tracing_target() {
        TracingTarget::Disabled | TracingTarget::NoOp => panic!(
            "{} should never be passed to this function",
            build_info.implementation.as_ref()
        ),
        TracingTarget::Stap => Box::new(target::stap::StapNativeCodeGenerator::new(
            out_dir, provider,
        )),
    }
}

fn cache_dir(out_dir: &Path) -> PathBuf {
    out_dir.join("cache")
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::testdata::*;
    use crate::TracingImplementation;

    #[test]
    fn caches_results() {
        // For each of our test crates, run the code generator twice.  Once with an empty cache,
        // and then again.  The first time should produce some output.  The second time should
        // produce no debug output for crates that are valid, but for crates with missing dependencies the
        // missing dependency error info should be output again
        for implementation in [TracingImplementation::StaticStap].iter() {
            let build_info = BuildInfo::new((*implementation).clone());
            let temp_dir = tempfile::tempdir().unwrap();
            let cache_dir = temp_dir.path().join("cache");

            for first_run in [true, false].into_iter() {
                //Generate code for all of the crates.
                for case in TEST_CRATES.iter() {
                    for target in case.targets.iter() {
                        let mut stdout = Vec::new();

                        process_file(
                            &build_info,
                            &mut stdout,
                            &cache_dir,
                            &case.root_directory.join(target.entrypoint),
                        );

                        let output = String::from_utf8(stdout).unwrap();

                        if *first_run {
                            // This is the first time generation ran, so there should definitely be
                            // some log output
                            assert_ne!("", output, "test crate {}", case.root_directory.display());
                        } else {
                            //expectation of output depends on whether or not we expect errors from
                            //resolving dependencies in this crate
                            let expected_errors = !target.expected_errors.is_empty();

                            if expected_errors {
                                assert_ne!(
                                    "",
                                    output,
                                    "test crate {}",
                                    case.root_directory.display()
                                );
                            } else {
                                //No errors are expected, so the only output should be the cargo
                                //commands to link to the native libraries.
                                let lines = output
                                    .lines()
                                    .filter(|line| {
                                        !(line.starts_with("cargo:rustc-link-lib")
                                            || line.starts_with("cargo:rustc-link-search"))
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                assert_eq!(
                                    "",
                                    lines,
                                    "test crate {}",
                                    case.root_directory.display()
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
