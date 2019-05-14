//! This module is responsible for generating the native C wrappers for each provider, which thunk
//! calls to the target platform's native tracing mechanism.  Most of the code generation logic is
//! the same across all platforms, with platform specific bits factored out into one of the
//! `platform` submodules
use crate::build_rs::BuildInfo;
use crate::cache;
use crate::deps::{self, SourceDependency};
use crate::gen::NativeLib;
use crate::spec::{self, ProviderSpecification};
use crate::{TracersError, TracersResult, TracingTarget, TracingType};
use failure::ResultExt;
use serde::{Deserialize, Serialize};
use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};

mod target;

/// The (possibly cached) data structure containing the results of processing a Rust source file
#[derive(Serialize, Deserialize)]
pub(crate) struct ProcessedFile {
    dependencies: Vec<SourceDependency>,
    providers: Vec<ProviderSpecification>,
}

/// The (possibly cached) data structure containing the results of running code gen on a provider
/// trait
#[derive(Serialize, Deserialize)]
pub(crate) struct ProcessedProviderTrait {
    pub native_libs: Vec<NativeLib>,
}

trait NativeCodeGenerator {
    /// Generates a native static library that wraps the platform-speciifc probing calls in
    /// something that Rust's FFI can handle
    fn generate_native_lib(&self) -> TracersResult<Vec<NativeLib>>;

    fn out_dir(&self) -> &Path;

    fn build_dir(&self) -> PathBuf {
        self.out_dir().join("build")
    }

    fn output_dir(&self) -> PathBuf {
        self.out_dir().join("output")
    }
}

const PROCESSED_PROVIDER_KEY: &str = "processed_provider";

/// Checks the cache to see if the provider described by `provider` has already been processed by
/// the native code generator and produced a native lib and Rust bindings.  If so returns the
/// details.  If not returns an error.
///
/// This is called from within the proc macros when they need to know about the generated bindings
/// for a given provider.
///
/// It assumes the environmant variable `OUT_DIR` is set to the output directory used when
/// `tracers_build::build()` was invoked in the caller's `build.rs` file.
pub(crate) fn get_processed_provider_info(
    provider: &ProviderSpecification,
) -> TracersResult<ProcessedProviderTrait> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").context("OUT_DIR")?);
    let cache_dir = cache::get_cache_path(&out_dir);
    cache::get_cached_object_computation(
        &cache_dir,
        provider.name(),
        provider.hash(),
        PROCESSED_PROVIDER_KEY,
    )
    .map_err(|e| TracersError::provider_trait_not_processed_error(provider.ident().to_string(), e))
}

pub(super) fn generate_native_code(
    build_info: &BuildInfo,
    stdout: &mut dyn Write,
    manifest_dir: &Path,
    out_dir: &Path,
    _package_name: &str,
    targets: Vec<PathBuf>,
) -> Vec<NativeLib> {
    assert!(build_info.implementation.tracing_type() == TracingType::Static);

    match build_info.implementation.tracing_target() {
        TracingTarget::Disabled | TracingTarget::NoOp => {
            writeln!(
                stdout,
                "No native code needed for {} tracing",
                build_info.implementation.tracing_target().as_ref()
            )
            .unwrap();
            vec![]
        }
        TracingTarget::Stap | TracingTarget::Lttng => {
            let mut libs = Vec::new();
            for target in targets.into_iter() {
                let target_path = manifest_dir.join(&target);
                writeln!(stdout, "Processing target {}", target_path.display()).unwrap();
                libs.append(&mut process_file(build_info, stdout, out_dir, &target_path));
            }

            libs
        }
    }
}

fn process_file(
    build_info: &BuildInfo,
    stdout: &mut dyn Write,
    out_dir: &Path,
    file: &Path,
) -> Vec<NativeLib> {
    //Find the dependent files and providers in this source file, retrieving that info from cache
    //if we've done this before
    let cache_dir = cache::get_cache_path(out_dir);
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
            let providers = spec::find_providers(&build_info.package_name, &file);

            Ok(ProcessedFile {
                dependencies,
                providers,
            })
        });

    match result {
        Ok(processed_file) => {
            //Maybe cached maybe not, we got the info for this file
            //Generate code for the providers, and recursively process all dependent files
            let mut libs = Vec::new();

            for dependency in processed_file.dependencies.into_iter() {
                match deps::resolve_dependency(file, &dependency) {
                    // Dependency resolved; recursively process this one also
                    Ok(dep_file) => {
                        libs.append(&mut process_file(build_info, stdout, out_dir, &dep_file))
                    }

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
                libs.append(&mut process_provider(build_info, stdout, out_dir, provider));
            }

            libs
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

            //On error there won't be any generated native libs obviously
            vec![]
        }
    }
}

fn process_provider(
    build_info: &BuildInfo,
    stdout: &mut dyn Write,
    out_dir: &Path,
    provider: ProviderSpecification,
) -> Vec<NativeLib> {
    let cache_dir = cache::get_cache_path(out_dir);

    // For this trait, generate native code for the probes.  If this trait was processed before
    // and hasn't changed, even if the source file it's in has changed, then we can skip that
    // generation and used the cached result
    let name = provider.name().to_owned();
    let ident = provider.ident().clone();
    let result = cache::cache_object_computation(
        &cache_dir,
        &name,
        provider.hash(),
        PROCESSED_PROVIDER_KEY,
        move || {
            let generator = create_native_code_generator(build_info, out_dir, provider);

            Ok(ProcessedProviderTrait {
                native_libs: generator.generate_native_lib()?,
            })
        },
    );

    match result {
        Ok(processed_provider) => {
            //Generation succeeded, so return the info to the caller.  It needs to be aggregated
            //and deduped before being printed out to cargo
            processed_provider.native_libs
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

            //No native libs generated in the error case
            vec![]
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
        TracingTarget::Lttng => Box::new(target::lttng::LttngNativeCodeGenerator::new(
            out_dir, provider,
        )),
    }
}

#[cfg(test)]
#[cfg(target_os = "linux")]
mod tests {
    use super::*;
    use crate::testdata;
    use crate::testdata::*;
    use crate::TracingImplementation;

    #[test]
    fn caches_results() {
        // For each of our test crates, run the code generator twice.  Once with an empty cache,
        // and then again.  The first time should produce some output.  The second time should
        // produce no debug output for crates that are valid, but for crates with missing dependencies the
        // missing dependency error info should be output again
        for implementation in [
            TracingImplementation::StaticStap,
            TracingImplementation::StaticLttng,
        ]
        .iter()
        {
            let build_info = BuildInfo::new(TEST_CRATE_NAME.to_owned(), (*implementation).clone());
            let temp_dir = tempfile::tempdir().unwrap();
            let out_dir = temp_dir.path().join("out");

            for first_run in [true, false].into_iter() {
                //Generate code for all of the crates.
                for case in TEST_CRATES.iter() {
                    let guard = testdata::with_env_vars(vec![
                        ("CARGO_PKG_NAME", case.package_name),
                        ("CARGO_MANIFEST_DIR", case.root_directory.to_str().unwrap()),
                        ("TARGET", "x86_64-linux-gnu"),
                        ("HOST", "x86_64-linux-gnu"),
                        ("OPT_LEVEL", "1"),
                        ("OUT_DIR", out_dir.to_str().unwrap()),
                    ]);

                    for target in case.targets.iter() {
                        let mut stdout = Vec::new();

                        process_file(
                            &build_info,
                            &mut stdout,
                            &out_dir,
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

                    drop(guard); //unset the env vars
                }
            }
        }
    }

    #[test]
    fn generates_processed_provider_trait() {
        //Run through all of our test traits, invoking the code generator for each
        //If the implementation is "real", not "disabled" or "noop", a library should be generated
        //and metadata about that library stored in the cache for proc macros to retrieve at
        //compile time.
        for test_trait in
            get_test_provider_traits(|t: &TestProviderTrait| t.expected_error.is_none())
        {
            let (attr, item_trait) = test_trait.get_attr_and_item_trait();
            let provider =
                ProviderSpecification::from_trait(testdata::TEST_CRATE_NAME, attr, item_trait)
                    .unwrap();

            //TODO: Run process_provider on each one, then verify the correct cargo commands are
            //output, and then call get_processed_provider_info to confirm the results are
            //persisted to the cache
            for implementation in [
                TracingImplementation::StaticStap,
                TracingImplementation::StaticLttng,
            ]
            .iter()
            {
                let build_info =
                    BuildInfo::new(TEST_CRATE_NAME.to_owned(), (*implementation).clone());
                let temp_dir = tempfile::tempdir().unwrap();
                let out_dir = temp_dir.path().join("out");
                let lib_dir = temp_dir.path().join("output");
                let guard = testdata::with_env_vars(vec![
                    ("TARGET", "x86_64-linux-gnu"),
                    ("HOST", "x86_64-linux-gnu"),
                    ("OPT_LEVEL", "1"),
                    ("OUT_DIR", out_dir.to_str().unwrap()),
                ]);
                let mut stdout = Vec::new();

                process_provider(&build_info, &mut stdout, &out_dir, provider.clone());

                let processed_provider = get_processed_provider_info(&provider)
                    .expect("There should be a processed provider");

                //There should at least be a static wrapper lib and static wrapper search path
                assert_eq!(
                    vec![
                        NativeLib::StaticWrapperLib(provider.name_with_hash()),
                        NativeLib::StaticWrapperLibPath(lib_dir)
                    ],
                    processed_provider
                        .native_libs
                        .into_iter()
                        .filter_map(|l| {
                            match l {
                                lib @ NativeLib::StaticWrapperLib(_) => Some(lib),
                                lib_path @ NativeLib::StaticWrapperLibPath(_) => Some(lib_path),
                                _ => None,
                            }
                        })
                        .collect::<Vec<_>>()
                );

                drop(guard);
            }
        }
    }
}
