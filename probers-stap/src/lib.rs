#![deny(warnings)]

pub mod tracer;
pub mod provider;
pub mod probe;

pub use tracer::*;
pub use provider::*;
pub use probe::*;

//#[cfg(test)]
//#[macro_use(quickcheck)]
//extern crate quickcheck_macros;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
