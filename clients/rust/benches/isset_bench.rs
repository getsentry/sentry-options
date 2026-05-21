use criterion::{Criterion, black_box, criterion_group, criterion_main};
use sentry_options::Options;
use std::path::Path;

const NS: &str = "sentry-options-testing";

fn opts() -> Options {
    Options::from_directory(Path::new("../../sentry-options")).unwrap()
}

fn bench_isset(c: &mut Criterion) {
    let opts = opts();
    let mut group = c.benchmark_group("isset");

    group.bench_function("set_key", |b| {
        b.iter(|| black_box(opts.isset(NS, "example-option").unwrap()));
    });

    group.bench_function("unset_key", |b| {
        b.iter(|| black_box(opts.isset(NS, "string-option").unwrap()));
    });

    group.finish();
}

criterion_group!(benches, bench_isset);
criterion_main!(benches);
