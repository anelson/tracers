use failure::{bail, format_err, Error, Fallible};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use syn::visit::Visit;

/// Struct which will implement `syn::visit::Visit` to efficiently scan through source files
/// looking for possible module references
struct RecursiveVisitor<'visitor, V: VisitFiles> {
    /// A stack of the source files currently being processed.  The element at index `0` is the
    /// initial source file, usually `src/lib.rs` or `src/bin.rs` or something like that.  Every
    /// subsequent element is a source file that was included into the previous source file by
    /// something like `mod` or `include!`.
    paths: Vec<PathBuf>,

    /// The visitor which will be called for all of the AST nodes in all of the processed source
    /// files
    visitor: &'visitor mut V,
}

#[allow(dead_code)]
pub(crate) fn recursively_visit<'visitor, V: VisitFiles>(
    source_file: &Path,
    visitor: &'visitor mut V,
) {
    let mut recursive_visitor = RecursiveVisitor::new(visitor);
    recursive_visitor.process_file(source_file)
}

fn parse_file(source_file: &Path) -> Fallible<syn::File> {
    let mut file = File::open(source_file)?;
    let mut src = String::new();
    file.read_to_string(&mut src)?;

    Ok(syn::parse_file(&src)?)
}

/// Given the path to a source file containing a `mod` item, and the name of the module, attempts
/// to find the source file which contains that module.  It does this by trying first to find a
/// file `$MODNAME.rs` in the same directory as `source_file`, and failing that will look for
/// `$MODNAME/mod.rs`.  If either of those exises, the path to the file is returned, but no attempt
/// is made to determine if it's valid Rust code.
///
/// If it fails, returns a descriptive error
fn find_module(source_file: &Path, module_name: &str) -> Fallible<PathBuf> {
    let mut path = source_file.to_owned();
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

impl<'visitor, V: VisitFiles> RecursiveVisitor<'visitor, V> {
    fn new(visitor: &'visitor mut V) -> RecursiveVisitor<'visitor, V> {
        RecursiveVisitor {
            paths: vec![],
            visitor: visitor,
        }
    }

    fn process_file(&mut self, source_file: &Path) {
        self.paths.push(source_file.to_owned());
        match parse_file(source_file) {
            Ok(file) => {
                // Pass the parsed AST for this file on to our visitor for it to do whatever it
                // needs to do
                self.visitor.visit_external_file(source_file, &file);

                // Now use our own visitor to look for references to more external files
                self.visit_file(&file)
            }
            Err(e) => self.visitor.external_file_error(source_file, &e),
        }
        self.paths.pop();
    }
}

impl<'ast, 'visitor, V: VisitFiles> Visit<'ast> for RecursiveVisitor<'visitor, V> {
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
            let source_file = self
                .paths
                .last()
                .expect("visit_item_mod called without any source paths");
            let module_name = i.ident.to_string();

            match find_module(source_file, &module_name) {
                Ok(module_path) => self.process_file(module_path.as_ref()),
                Err(e) => self
                    .visitor
                    .external_file_error(source_file, &format_err!("{}: {}", module_name, e)),
            }
        }
    }
}

/// The `syn` crate's `VisitFiles` trait provides the Visitor pattern as applied to the AST of a single
/// Rust source file.  This extends that concept to multiple Rust code files which together
/// comprise a crate.
///
/// For example, consider this code:
///
/// ```
/// // src/lib.rs
/// mod foo;
/// mod bar;
///
/// //src/foo.rs
/// struct Foo{}
///
/// //src/bar/mod.rs
/// struct Bar{}
/// ```
///
/// If the contents of `src/lib.rs` are passed to an implementation of this trait via
/// `visit_external_file`,
/// this implementation will process each of the `mod` declarations, locate and parse the
/// implementation files, and continue to execute the `VisitFiles` trait methods on the module's
/// contents, recursively.
///
/// When a module's implementation file is successfully located, `visit_external_file` is called.
///
/// If for some reason the referenced module cannot be found or fails to open,
/// `external_file_error` is called with the details of the error.
pub(crate) trait VisitFiles {
    fn visit_external_file(&mut self, path: &Path, contents: &syn::File);

    /// Called when an AST element references an external file but that file cannot be located or
    /// if there's an error opening or parsing it.
    fn external_file_error(&mut self, path: &Path, error: &Error);
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::testdata::*;

    /// Simple implementation of the visitor that just records all of the files that were
    /// discovered, and all errors reported
    struct TestVisitor {
        pub paths: Vec<PathBuf>,
        pub errors: Vec<(PathBuf, String)>,
    }

    impl VisitFiles for TestVisitor {
        fn visit_external_file(&mut self, path: &Path, _contents: &syn::File) {
            self.paths.push(path.to_owned());
        }

        /// Called when an AST element references an external file but that file cannot be located or
        /// if there's an error opening or parsing it.
        fn external_file_error(&mut self, path: &Path, error: &Error) {
            self.errors.push((path.to_owned(), error.to_string()));
        }
    }

    #[test]
    fn test_case() {
        for case in TEST_CASES.iter() {
            for target in case.targets.iter() {
                let mut visitor = TestVisitor {
                    paths: Vec::new(),
                    errors: Vec::new(),
                };
                let entrypoint = case.root_directory.join(target.entrypoint);
                recursively_visit(&entrypoint, &mut visitor);

                let mut expected_paths = vec![entrypoint];
                let mut additional_paths: Vec<_> = target
                    .additional_source_files
                    .iter()
                    .map(|p| case.root_directory.join(p))
                    .collect();
                expected_paths.append(&mut additional_paths);

                assert_eq!(expected_paths, visitor.paths);

                //Go through the expected errors, and make sure they are all present.
                //When we find an expected error, we'll remove it from the list of errors.
                //Then at the end, we'll assert the errors array is empty and any remaining errors
                //will cause that to fail
                let unexpected_errors = visitor
                    .errors
                    .iter()
                    .filter(|(file, error_msg)| {
                        //Look in the expected errors; was this one expected?
                        !target
                            .expected_errors
                            .iter()
                            .any(|(expected_file, expected_substring)| {
                                &case.root_directory.join(expected_file) == file
                                    && error_msg.contains(expected_substring)
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
                        !visitor.errors.iter().any(|(file, error_msg)| {
                            &case.root_directory.join(expected_file) == file
                                && error_msg.contains(expected_substring)
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
