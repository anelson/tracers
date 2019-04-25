//! This module contains logic that looks into the AST of a Rust source file and finds dependent
//! source files.  By "dependent" is meant source file dependencies within the crate, primarily
//! `mod` statements incorporating submodules, but this will also at some point try to follow
//! `include!` macros where possible.
use failure::{bail, Fallible};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use syn::visit::Visit;

/// The type of source dependency.
#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum SourceDependency {
    /// A `mod` dependency, specifying the name of the module.
    Mod(String),
}

/// Scans an already-parsed AST and finds the source dependencies within the file
pub(crate) fn get_dependencies(contents: &syn::File) -> Vec<SourceDependency> {
    let mut visitor = Visitor::new();

    visitor.visit_file(contents);

    visitor.deps
}

/// Given the path to a source file and a previously-discovered dependency, attempts to resolve
/// that dependency to an existing source file.
pub(crate) fn resolve_dependency(source_path: &Path, dep: &SourceDependency) -> Fallible<PathBuf> {
    match dep {
        SourceDependency::Mod(module_name) => find_module(source_path, &module_name),
    }
}

/// Given the path to a source file containing a `mod` item, and the name of the module, attempts
/// to find the source file which contains that module.  It does this by trying first to find a
/// file `$MODNAME.rs` in the same directory as `source_path`, and failing that will look for
/// `$MODNAME/mod.rs`.  If either of those exises, the path to the file is returned, but no attempt
/// is made to determine if it's valid Rust code.
///
/// If it fails, returns a descriptive error
fn find_module(source_path: &Path, module_name: &str) -> Fallible<PathBuf> {
    let mut path = source_path.to_owned();
    //Pop the file name so we have just the directory path
    path.pop();
    path.push(format!("{}.rs", module_name));
    if path.exists() {
        return Ok(path);
    }
    path.pop();
    path.push(format!("{}/mod.rs", module_name));
    if path.exists() {
        return Ok(path);
    }

    //Else, could not find the module's source code
    bail!("Unable to locate source code for module '{}'", module_name);
}

/// Simple implementation of the `Visit` trait provided by `syn`, to traverse the AST of a single
/// source file, looking for tokens that indicate an external reference
struct Visitor {
    deps: Vec<SourceDependency>,
}

impl Visitor {
    fn new() -> Visitor {
        Visitor { deps: vec![] }
    }
}

impl<'ast> Visit<'ast> for Visitor {
    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        //First call the default implementation.
        syn::visit::visit_item_mod(self, i);

        //Now look at this mod statement.  If it has an implementation, then we're done.  If not,
        //if it looks something like:
        //```
        //mod foo;
        //```
        //
        //Then it actually works a bit like a #include in C.  Try to find the source file for that
        //module, relative to the current source file, which should be the last element in the
        //`paths` member.
        if i.content == None {
            let mut module_name = i.ident.to_string();

            //Rust allows module names which are also Rust reserved words to be escaped with `r#`,
            //for example:
            //
            //```
            //mod r#static //this is in `static.rs` or `static/mod.rs`
            //```
            let module_name = if module_name.starts_with("r#") {
                module_name.split_off(2)
            } else {
                module_name
            };

            self.deps.push(SourceDependency::Mod(module_name));
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::testdata::*;
    use failure::Error;
    use std::fs::File;
    use std::io::Read;

    fn parse(source_path: &Path) -> Fallible<syn::File> {
        let mut file = File::open(source_path)?;
        let mut src = String::new();
        file.read_to_string(&mut src)?;

        Ok(syn::parse_file(&src)?)
    }

    fn find_sources(entrypoint: &Path) -> (Vec<PathBuf>, Vec<(PathBuf, Error)>) {
        let mut source_paths: Vec<PathBuf> = Vec::new();
        let mut source_errors: Vec<(PathBuf, Error)> = Vec::new();

        match parse(entrypoint) {
            Ok(ast) => {
                source_paths.push(entrypoint.to_owned());

                let deps = get_dependencies(&ast);

                for dep in deps {
                    match resolve_dependency(entrypoint, &dep) {
                        Ok(dep_path) => {
                            let (mut dep_paths, mut dep_errors) = find_sources(&dep_path);
                            source_paths.append(&mut dep_paths);
                            source_errors.append(&mut dep_errors);
                        }
                        Err(e) => source_errors.push((entrypoint.to_owned(), e)),
                    }
                }
            }
            Err(e) => source_errors.push((entrypoint.to_owned(), e)),
        }

        (source_paths, source_errors)
    }

    #[test]
    fn test_case() {
        for case in TEST_CRATES.iter() {
            for target in case.targets.iter() {
                let entrypoint = case.root_directory.join(target.entrypoint);
                let (paths, errors) = find_sources(&entrypoint);

                let mut expected_paths = vec![entrypoint];
                let mut additional_paths: Vec<_> = target
                    .additional_source_files
                    .iter()
                    .map(|p| case.root_directory.join(p))
                    .collect();
                expected_paths.append(&mut additional_paths);

                assert_eq!(expected_paths, paths);

                //Make sure all expected errors were reported, and that no other, unexpected errors
                //were
                let unexpected_errors = errors
                    .iter()
                    .filter(|(file, error_msg)| {
                        //Look in the expected errors; was this one expected?
                        !target
                            .expected_errors
                            .iter()
                            .any(|(expected_file, expected_substring)| {
                                &case.root_directory.join(expected_file) == file
                                    && error_msg.to_string().contains(expected_substring)
                            })
                    })
                    .map(|(file, error_msg)| format!("{}: {}", file.to_str().unwrap(), error_msg))
                    .collect::<Vec<_>>();
                assert_eq!(
                    Vec::<String>::new(),
                    unexpected_errors,
                    "Some unexpected errors were reported"
                );;
                let missing_errors = target
                    .expected_errors
                    .iter()
                    .filter(|(expected_file, expected_substring)| {
                        !errors.iter().any(|(file, error_msg)| {
                            &case.root_directory.join(expected_file) == file
                                && error_msg.to_string().contains(expected_substring)
                        })
                    })
                    .map(|(expected_file, expected_substring)| {
                        format!("{}: {}", expected_file, expected_substring)
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    Vec::<String>::new(),
                    missing_errors,
                    "Some expected errors were not reported"
                );;
            }
        }
    }
}
