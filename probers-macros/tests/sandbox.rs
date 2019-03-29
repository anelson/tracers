use probers_macros::prober;

#[prober]
trait Foo {
    fn probe0();
    fn probe1(arg0: &str);
    fn probe2(arg0: &str, arg1: &str, arg2: usize, arg3: &Option<String>);
}

pub fn main() {
    probe! { Foo::probe0() };

    probe! {
        println!    ("This is also about to be printed");

        dbg!(Foo::probe0());
    }
    Foo::probe0_impl();
    Foo::probe1_impl("foo!");
    Foo::probe2_impl("foo!", "bar!", 5, &Some("baz!".to_string()));
}
