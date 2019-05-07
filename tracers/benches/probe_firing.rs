use criterion::{black_box, Criterion, criterion_group, criterion_main};
use tracers_macros::{probe, tracer};

#[tracer]
trait ProbeBenchmarks {
    fn no_args();
    fn int_arg(arg0: usize);
    fn string_arg(arg0: &str);
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("fire no_args", |b| {
        b.iter(|| black_box(probe!(ProbeBenchmarks::no_args())))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
