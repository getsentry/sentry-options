use std::time::{Duration, Instant};

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use sentry_options::{init_with_schemas, options};
use serde_json::Value;

const SCHEMA: &str = r#"{
    "version": "1.0",
    "type": "object",
    "properties": {
        "enabled": {
            "type": "boolean",
            "default": false,
            "description": "Enable feature"
        }
    }
}"#;

fn setup() {
    let _ = init_with_schemas(&[("test", SCHEMA)]);
}

struct CachedOption {
    cached: Value,
    last_checked: Instant,
    ttl: Duration,
}

impl CachedOption {
    fn new(initial: Value, ttl: Duration) -> Self {
        Self {
            cached: initial,
            last_checked: Instant::now(),
            ttl,
        }
    }

    fn get(&mut self) -> Value {
        if self.last_checked.elapsed() >= self.ttl {
            if let Ok(v) = options("test").and_then(|o| o.get("enabled")) {
                self.cached = v;
            }
            self.last_checked = Instant::now();
        }
        self.cached.clone()
    }
}

fn bench_direct(c: &mut Criterion) {
    setup();

    c.bench_function("options(ns).get(key)", |b| {
        b.iter(|| black_box(options("test").unwrap().get("enabled").unwrap()))
    });
}

fn bench_cached(c: &mut Criterion) {
    setup();

    let initial = options("test").unwrap().get("enabled").unwrap();
    let mut cached = CachedOption::new(initial, Duration::from_secs(5));

    c.bench_function("cached get() [TTL=5s, fast path]", |b| {
        b.iter(|| black_box(cached.get()))
    });
}

fn bench_isset(c: &mut Criterion) {
    setup();

    c.bench_function("options(ns).isset(key)", |b| {
        b.iter(|| black_box(options("test").unwrap().isset("enabled").unwrap()))
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().measurement_time(Duration::from_secs(15));
    targets = bench_direct, bench_cached, bench_isset
}
criterion_main!(benches);
