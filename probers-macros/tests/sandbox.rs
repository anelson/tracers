use probers_macros::prober;

#[prober]
trait Foo {
    //type Bar;

    fn probe0(&self);
    fn probe1(&self, arg0: &str);
}
