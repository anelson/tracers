//! Custom build logic that uses the features enabled by the dependent crate to determine which
//! tracing implementation to compile with

fn main() {
    tracers_build::tracers_build()
}
