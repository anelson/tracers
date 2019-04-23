//! Custom build logic that uses the features enabled by the dependent crate to determine which
//! tracing implementation to compile with
use probers_build::build_rs::probers_build;

fn main() {
    probers_build()
}
