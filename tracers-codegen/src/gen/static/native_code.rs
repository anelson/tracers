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
use failure::ResultExt;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The (possibly cached) data structure containing the results of processing a Rust source file
#[derive(Serialize, Deserialize)]
struct ProcessedFile {
    dependencies: Vec<SourceDependency>,
    providers: Vec<ProviderSpecification>,
}

pub(super) fn generate_native_code(
    build_info: &BuildInfo,
    stdout: &mut dyn Write,
    manifest_dir: &Path,
    cache_dir: &Path,
    _package_name: &str,
    targets: Vec<PathBuf>,
) {
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
                process_file(build_info, stdout, cache_dir, &target_path);
            }
        }
    };
}

fn process_file(build_info: &BuildInfo, stdout: &mut dyn Write, cache_dir: &Path, file: &Path) {
    //Find the dependent files and providers in this source file, retrieving that info from cache
    //if we've done this before
    let processed_file =
        cache::cache_file_computation(cache_dir, file, "processed-file", |file_contents| {
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
        })
        .map_err(|e| {
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
        })
        .ok();

    if let Some(processed_file) = processed_file {
        //Maybe cached maybe not, we got the info for this file
        //Generate code for the providers, and recursively process all dependent files
        for dependency in processed_file.dependencies.into_iter() {
            match deps::resolve_dependency(file, &dependency) {
                // Dependency resolved; recursively process this one also
                Ok(dep_file) => process_file(build_info, stdout, cache_dir, &dep_file),

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
            let _dontcare = process_provider(build_info, stdout, cache_dir, file, &provider)
                .map_err(|e| {
                    writeln!(
                        stdout,
                        "cargo:WARNING=Error generating tracing code for '{}': {}",
                        provider.ident(),
                        e
                    )
                    .unwrap();
                    writeln!(
                        stdout,
                        "cargo:WARNING=Tracing may not be available for {}",
                        provider.ident()
                    )
                    .unwrap();
                });
        }
    }
}

fn process_provider(
    _build_info: &BuildInfo,
    _stdout: &mut dyn Write,
    _cache_dir: &Path,
    _file: &Path,
    _provider: &ProviderSpecification,
) -> TracersResult<()> {
    unimplemented!()
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
        // produce nothing for crates that are valid, but for crates with missing dependencies the
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
                                assert_eq!(
                                    "",
                                    output,
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
