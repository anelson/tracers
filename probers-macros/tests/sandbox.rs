use probers_macros::prober;

//#[prober]
trait Foo {
    fn probe0();
    fn probe1(arg0: &str);
    fn probe2(arg0: &str, arg1: &str, arg2: usize, arg3: &Option<String>);
    //fn probe_fuck(arg: Option<&str>);
}
