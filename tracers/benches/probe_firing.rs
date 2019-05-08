use criterion::{black_box, criterion_group, criterion_main, Bencher, Criterion, Fun};
use tracers_macros::{init_provider, probe, tracer};

static INT_ARG: usize = 324;
static STRING_ARG: &str =
    "fear is the mind killer I will face my fear I will let it pass through me";

#[tracer]
trait ProbeBenchmarks {
    fn no_args();
    fn int_arg1(arg0: usize);
    fn int_arg3(arg0: usize, arg1: usize, arg2: usize);
    fn int_arg6(arg0: usize, arg1: usize, arg2: usize, arg3: usize, arg4: usize, arg5: usize);
    fn int_arg12(
        arg0: usize,
        arg1: usize,
        arg2: usize,
        arg3: usize,
        arg4: usize,
        arg5: usize,
        arg6: usize,
        arg7: usize,
        arg8: usize,
        arg9: usize,
        arg10: usize,
        arg11: usize,
    );
    fn string_arg1(arg0: &str);
    fn string_arg3(arg0: &str, arg1: &str, arg2: &str);
    fn string_arg6(arg0: &str, arg1: &str, arg2: &str, arg3: &str, arg4: &str, arg5: &str);
    fn string_arg12(
        arg0: &str,
        arg1: &str,
        arg2: &str,
        arg3: &str,
        arg4: &str,
        arg5: &str,
        arg6: &str,
        arg7: &str,
        arg8: &str,
        arg9: &str,
        arg10: &str,
        arg11: &str,
    );
}

fn bench_no_args(b: &mut Bencher, _arg: &()) {
    b.iter(|| probe!(ProbeBenchmarks::no_args()))
}

fn bench_int_arg1(b: &mut Bencher, _arg: &()) {
    b.iter(|| probe!(ProbeBenchmarks::int_arg1(black_box(INT_ARG))))
}

fn bench_int_arg3(b: &mut Bencher, _arg: &()) {
    b.iter(|| {
        probe!(ProbeBenchmarks::int_arg3(
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG)
        ))
    })
}

fn bench_int_arg6(b: &mut Bencher, _arg: &()) {
    b.iter(|| {
        probe!(ProbeBenchmarks::int_arg6(
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG)
        ))
    })
}

fn bench_int_arg12(b: &mut Bencher, _arg: &()) {
    b.iter(|| {
        probe!(ProbeBenchmarks::int_arg12(
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG),
            black_box(INT_ARG)
        ))
    })
}

fn bench_string_arg1(b: &mut Bencher, _arg: &()) {
    b.iter(|| probe!(ProbeBenchmarks::string_arg1(black_box(STRING_ARG))))
}

fn bench_string_arg3(b: &mut Bencher, _arg: &()) {
    b.iter(|| {
        probe!(ProbeBenchmarks::string_arg3(
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG)
        ))
    })
}

fn bench_string_arg6(b: &mut Bencher, _arg: &()) {
    b.iter(|| {
        probe!(ProbeBenchmarks::string_arg6(
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG)
        ))
    })
}

fn bench_string_arg12(b: &mut Bencher, _arg: &()) {
    b.iter(|| {
        probe!(ProbeBenchmarks::string_arg12(
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG),
            black_box(STRING_ARG)
        ))
    })
}

fn bench_fire_disabled(c: &mut Criterion) {
    let funcs = vec![
        Fun::new("no args", bench_no_args),
        Fun::new("int_arg1", bench_int_arg1),
        Fun::new("int_arg3", bench_int_arg3),
        Fun::new("int_arg6", bench_int_arg6),
        Fun::new("int_arg12", bench_int_arg12),
        Fun::new("string_arg1", bench_string_arg1),
        Fun::new("string_arg3", bench_string_arg3),
        Fun::new("string_arg6", bench_string_arg6),
        Fun::new("string_arg12", bench_string_arg12),
    ];

    let provider_info = init_provider!(ProbeBenchmarks).expect("Provider init failed");

    c.bench_functions(
        &format!("Firing disabled probes on '{}'", provider_info),
        funcs,
        (),
    );
}

criterion_group!(benches, bench_fire_disabled);
criterion_main!(benches);
