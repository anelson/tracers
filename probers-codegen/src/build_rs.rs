//! This module contains the code that is used within a dependent crate's `build.rs` file to select
//! the suitable tracing implementation at build time.
use crate::TracingImplementation;
use failure::{bail, format_err, Fallible};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

/// Captures the features enabled for the build.  There are various combinations of them which
/// influence the logic related to what implementation is preferred
#[derive(Debug)]
pub struct FeatureFlags {
    enable_dynamic_tracing: bool,
    enable_native_tracing: bool,
    force_dyn_stap: bool,
    force_dyn_noop: bool,
}

impl FeatureFlags {
    /// Read the feature flags from the environment variables set by Cargo at build time.
    ///
    /// Fails with an error if the combination of features is not valid
    pub fn from_env() -> Fallible<FeatureFlags> {
        Self::new(
            Self::is_feature_enabled("enable-dynamic-tracing"),
            Self::is_feature_enabled("enable-native-tracing"),
            Self::is_feature_enabled("force-dyn-stap"),
            Self::is_feature_enabled("force-dyn-noop"),
        )
    }

    /// Creates a feature flag structure from explicit arguments.  Mostly used for testing
    pub fn new(
        enable_dynamic_tracing: bool,
        enable_native_tracing: bool,
        force_dyn_stap: bool,
        force_dyn_noop: bool,
    ) -> Fallible<FeatureFlags> {
        if enable_dynamic_tracing && enable_native_tracing {
            bail!("The features `enable-dynamic-tracing` and `enable-native-tracing` are mutually exclusive; please choose one")
        }

        if force_dyn_stap && force_dyn_noop {
            bail!("The features `force-dyn-stap` and `force_dyn_noop` are mutually exclusive; please choose one")
        }

        Ok(FeatureFlags {
            enable_dynamic_tracing,
            enable_native_tracing,
            force_dyn_stap,
            force_dyn_noop,
        })
    }

    pub fn enable_tracing(&self) -> bool {
        self.enable_dynamic() || self.enable_native()
    }

    pub fn enable_dynamic(&self) -> bool {
        self.enable_dynamic_tracing || self.force_dyn_noop || self.force_dyn_stap
    }

    pub fn enable_native(&self) -> bool {
        self.enable_native_tracing
    }

    pub fn force_dyn_stap(&self) -> bool {
        //Should the dynamic stap be required on pain of build failure?
        self.force_dyn_stap
    }

    pub fn force_dyn_noop(&self) -> bool {
        //Should the dynamic stap be required on pain of build failure?
        self.force_dyn_noop
    }

    fn is_feature_enabled(name: &str) -> bool {
        env::var(&format!(
            "CARGO_FEATURE_{}",
            name.to_uppercase().replace("-", "_")
        ))
        .is_ok()
    }
}

/// Serializable struct which is populated in `build.rs` to indicate to the proc macros which
/// tracing implementation they should use.
#[derive(Debug, Serialize, Deserialize)]
pub struct BuildInfo {
    pub implementation: TracingImplementation,
}

impl BuildInfo {
    pub fn new(implementation: TracingImplementation) -> BuildInfo {
        BuildInfo { implementation }
    }

    pub fn load() -> Fallible<BuildInfo> {
        let path = Self::get_build_path()?;
        let file = File::open(&path).map_err(|e| {
            //Create a more helpful message here
            format_err!("Unable to read build info from '{}'.\nAre you sure you're calling `probers_build::build()` in your `build.rs`?\nError cause: {}",
            path.display(), e)
        })?;
        let reader = BufReader::new(file);

        serde_json::from_reader(reader).map_err(std::convert::Into::into) //convert the error to a failure-compatible type
    }

    pub fn save(&self) -> Fallible<()> {
        let path = Self::get_build_path()?;

        //Make sure the directory exists
        path.parent()
            .map(|p| {
                std::fs::create_dir_all(p).map_err(|e| {
                    format_err!("Error creating output directory {}: {}", p.display(), e)
                })
            })
            .unwrap_or(Ok(()))?;

        let file = File::create(&path)
            .map_err(|e| format_err!("Error creating build info file {}: {}", path.display(), e))?;
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, self)
            .map_err(|e| format_err!("Error saving cached results to {}: {}", path.display(), e))
    }

    fn get_build_path() -> Fallible<PathBuf> {
        let rel_path = PathBuf::from(&format!(
            "{}-{}/buildinfo.json",
            env::var("CARGO_PKG_NAME")?,
            env::var("CARGO_PKG_VERSION")?
        ));
        Ok(PathBuf::from(env::var("OUT_DIR")?).join(rel_path))
    }
}
