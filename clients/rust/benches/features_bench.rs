use criterion::{Criterion, black_box, criterion_group, criterion_main};
use sentry_options::{FeatureChecker, FeatureContext, Options};
use serde_json::json;
use std::path::Path;

const NS: &str = "sentry-options-testing";

fn opts() -> &'static Options {
    Box::leak(Box::new(
        Options::from_directory(Path::new("../../sentry-options")).unwrap(),
    ))
}

fn bench_has(c: &mut Criterion) {
    let opts = opts();
    let checker = FeatureChecker::new(NS.to_string(), opts);
    let mut group = c.benchmark_group("has");

    group.bench_function("enabled_100pct", |b| {
        let mut ctx = FeatureContext::new();
        ctx.insert("organization_id", json!(123));
        b.iter(|| black_box(checker.has("organizations:enabled-feature", &ctx)));
    });

    group.bench_function("disabled", |b| {
        let mut ctx = FeatureContext::new();
        ctx.insert("organization_id", json!(123));
        b.iter(|| black_box(checker.has("organizations:disabled-feature", &ctx)));
    });

    group.bench_function("rollout_zero", |b| {
        let mut ctx = FeatureContext::new();
        ctx.insert("organization_id", json!(123));
        b.iter(|| black_box(checker.has("organizations:rollout-zero", &ctx)));
    });

    group.bench_function("rollout_50pct", |b| {
        let mut ctx = FeatureContext::new();
        ctx.insert("is_trial", json!(true));
        b.iter(|| black_box(checker.has("organizations:rollout-mid", &ctx)));
    });

    group.bench_function("no_match", |b| {
        let mut ctx = FeatureContext::new();
        ctx.insert("organization_id", json!(999));
        b.iter(|| black_box(checker.has("organizations:enabled-feature", &ctx)));
    });

    group.bench_function("undefined", |b| {
        let ctx = FeatureContext::new();
        b.iter(|| black_box(checker.has("organizations:nonexistent", &ctx)));
    });

    group.finish();
}

criterion_group!(benches, bench_has);
criterion_main!(benches);
