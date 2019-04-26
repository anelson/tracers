use tracers_macros::probe;
pub mod child_module;

pub fn something_else() {
    probe!(child_module::MyTraceProvider::something_else("foo", 3));
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
