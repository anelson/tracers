extern crate probers;

use nom::*;
use probers::init_provider;
use probers::probe;
use probers::prober;
use std::io::prelude::*;

/// This is a probe provider which is used to exercise the probing infrastructure with a few
/// different combinations of arguments.
#[prober]
pub trait TestProbes {
    fn probe0();
    fn probe1(text: &str);
    fn probe2(text: &str, number: usize);
    fn probe3(text: &str, number: usize, opt: &Option<&String>);
}

#[derive(Debug, PartialEq)]
enum ProbeType {
    Probe0,
    Probe1 {
        text: String,
    },
    Probe2 {
        text: String,
        number: usize,
    },
    Probe3 {
        text: String,
        number: usize,
        opt: Option<String>,
    },
}

named!(
    quoted_string<&str, String>,
    map!(
        delimited!(tag!("\""), take_until!("\""), tag!("\"")),
        |s| String::from(s)
    )
);
named!(number<&str, usize>, map_res!(take_while!(|c: char| c.is_ascii_digit()), |num: &str| num.parse::<usize>()));
named!(optional_string<&str, Option<String>>, opt!(complete!(quoted_string)));
named!(eol<&str, &str>, tag!(";"));

named!(
    probe0<&str, ProbeType>,
    do_parse!(tag!("probe0") >> eol >> (ProbeType::Probe0))
);

named!(
    probe1<&str, ProbeType>,
    do_parse!(
        tag!("probe1") >>
        sp >>
        text: quoted_string >>
        eol >> (
            ProbeType::Probe1 { text }
            )
        )
    );

named!(
    probe2<&str, ProbeType>,
    do_parse!(
        tag!("probe2") >>
        sp >>
        text: quoted_string >>
        sp >>
        number: number >>
        eol >> (
            ProbeType::Probe2 { text, number }
            )
        )
    );

named!(
    probe3<&str, ProbeType>,
    do_parse!(
        tag!("probe3") >>
        sp >>
        text: quoted_string >>
        sp >>
        number: number >>
        sp >>
        opt: optional_string >>
        eol >> (
            ProbeType::Probe3 { text, number, opt }
            )
        )
    );

named!(
    probe_command<&str, ProbeType>,
    complete! (
    alt! (
        probe0 | probe1 | probe2 | probe3
        )
    )
    );

fn fire_probe(pt: ProbeType) {
    dump_status();
    match pt {
        ProbeType::Probe0 => probe!(TestProbes::probe0()),
        ProbeType::Probe1 { text } => probe!(TestProbes::probe1(&text)),
        ProbeType::Probe2 { text, number } => probe!(TestProbes::probe2(&text, number)),
        ProbeType::Probe3 { text, number, opt } => {
            probe!(TestProbes::probe3(&text, number, &opt.as_ref()))
        }
    }
}

fn dump_status() {
    println!("Probe status: probe0:{} probe1:{} probe2:{} probe3:{}", 
             TestProbes::probe0_enabled(),
             TestProbes::probe1_enabled(),
             TestProbes::probe2_enabled(),
             TestProbes::probe3_enabled());
}

fn main() {
    println!("Initializing the probe provider");
    if let Some(err) = init_provider!(TestProbes) {
        panic!("Probe provider initialization failed: {}", err);
    }
    println!("Probe provider initialized");
    dump_status();

    for line in std::io::stdin().lock().lines() {
        let line = line.expect("Error reading line from stdin");
        //Implement a primitive parser to parse commands.  Each command is the name of a probe to
        //fire and then its arguments.
        match probe_command(&line) {
            Ok((_, pt)) => fire_probe(pt),
            Err(e) => panic!("Invalid input '{}': {}", line, e),
        }
    }
}

