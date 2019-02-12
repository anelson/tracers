pub mod argtypes;
pub use crate::argtypes::int::*;
pub use crate::argtypes::*;

#[cfg(test)]
mod tests {
    use super::argtypes::*;

    #[test]
    fn it_works() {
        let somearg = 5u64;
        probe(somearg);

        assert_eq!(2 + 2, 4);
    }

    fn probe<A: ProbeArgType<A>>(arg: A) -> () {
        let mut wrapper = wrap(arg);
        probe2(&mut wrapper);
    }

    fn probe2<A: ProbeArgType<A>, W: ProbeArgWrapper<A>>(arg: &mut W) -> () {
        let _c_type = arg.to_c_type();
        println!("C type is {:?}", arg);
    }
}
