use complexlib::VariousProbes;
use tracers_macros::probe;

fn main() {
    probe!(VariousProbes::bin1_start());
    println!("bin1");
    probe!(VariousProbes::bin1_end());
}
