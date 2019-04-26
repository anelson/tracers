use tracers_macros::tracer;

#[tracer]
pub trait VariousProbes {
    fn bin1_start();
    fn bin1_end();

    fn bin2_start();
    fn bin2_end();
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
