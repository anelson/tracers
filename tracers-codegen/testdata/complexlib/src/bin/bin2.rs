use complexlib::VariousProbes;
use tracers_macros::probe;

fn main() {
    probe!(VariousProbes::bin2_start());
    println!("bin2");
    probe!(VariousProbes::bin2_end());
}
