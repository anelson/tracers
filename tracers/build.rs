//! Custom build logic that uses the features enabled by the dependent crate to determine which
//! tracing implementation to compile with
use tracers_build::build_rs::tracers_build;

fn main() {
    tracers_build()
}
