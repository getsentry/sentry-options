use criterion::{Criterion, black_box, criterion_group, criterion_main};
use sentry_options::{ExperimentChecker, ExperimentContext, Options};
use serde_json::json;
use std::path::Path;

const NS: &str = "sentry-options-testing";

fn opts() -> &'static Options {
    Box::leak(Box::new(
        Options::from_directory(Path::new("../../sentry-options")).unwrap(),
    ))
}

fn bench_assign(c: &mut Criterion) {
    let checker = ExperimentChecker::new(NS.to_string(), opts());
    let mut ctx = ExperimentContext::new();
    ctx.insert("organization_id".to_string(), json!(123));

    c.bench_function("assign", |b| {
        b.iter(|| black_box(checker.assign("checkout-color", &ctx)));
    });
}

criterion_group!(benches, bench_assign);
criterion_main!(benches);
