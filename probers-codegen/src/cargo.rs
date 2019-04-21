use cargo_metadata::MetadataCommand;
use failure::{format_err, Fallible};
use std::path::PathBuf;

/// Given the path to a Cargo manifest and the name of a package, invokes `cargo metadata` and
/// parses the output to find all of the targets of that package.
///
/// If successful, returns a `Vec` of `Path`s, each the entry point of one of the package's
/// targets.
pub(crate) fn get_targets(manifest_path: &str, package_name: &str) -> Fallible<Vec<PathBuf>> {
    let mut cmd = MetadataCommand::new();
    let metadata = cmd
        .manifest_path(&manifest_path)
        .no_deps()
        .exec()
        .map_err(|e| format_err!("cargo metadata invocation failed: {}", e))?;

    //Find this specific package
    let package = metadata
        .packages
        .iter()
        .find(|p| p.name == package_name)
        .ok_or_else(|| format_err!("Unable to find package {} in cargo metadata", package_name))?;

    Ok(package.targets.iter().map(|t| t.src_path.clone()).collect())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::testdata::*;

    #[test]
    fn gets_correct_targets() {
        for case in TEST_CRATES.iter() {
            let mut expected_targets: Vec<_> = case
                .targets
                .iter()
                .map(|t| case.root_directory.join(t.entrypoint))
                .collect();
            let mut targets = get_targets(
                case.root_directory
                    .join(PathBuf::from("Cargo.toml"))
                    .to_str()
                    .unwrap(),
                case.package_name,
            )
            .unwrap();

            expected_targets.sort();
            targets.sort();

            assert_eq!(expected_targets, targets);
        }
    }
}
