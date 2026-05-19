use criterion::{Criterion, black_box, criterion_group, criterion_main};
use sentry_options::Options;
use std::path::Path;

const NS: &str = "sentry-options-testing";

fn opts() -> Options {
    Options::from_directory(Path::new("../../sentry-options")).unwrap()
}

fn bench_get(c: &mut Criterion) {
    let opts = opts();
    let mut group = c.benchmark_group("get");

    group.bench_function("string", |b| {
        b.iter(|| black_box(opts.get(NS, "example-option").unwrap()));
    });
    group.bench_function("integer", |b| {
        b.iter(|| black_box(opts.get(NS, "int-option").unwrap()));
    });
    group.bench_function("boolean", |b| {
        b.iter(|| black_box(opts.get(NS, "bool-option").unwrap()));
    });
    group.bench_function("float", |b| {
        b.iter(|| black_box(opts.get(NS, "float-option").unwrap()));
    });
    group.bench_function("object", |b| {
        b.iter(|| black_box(opts.get(NS, "object-option").unwrap()));
    });
    group.bench_function("array", |b| {
        b.iter(|| black_box(opts.get(NS, "array-option").unwrap()));
    });
    group.bench_function("default_fallback", |b| {
        b.iter(|| black_box(opts.get(NS, "string-option").unwrap()));
    });

    group.finish();
}

criterion_group!(benches, bench_get);
criterion_main!(benches);
