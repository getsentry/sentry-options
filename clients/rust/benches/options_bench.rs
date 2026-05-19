use std::fs;
use std::path::Path;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use sentry_options::Options;

fn create_schema(dir: &Path, namespace: &str, schema: &str) {
    let schema_dir = dir.join(namespace);
    fs::create_dir_all(&schema_dir).unwrap();
    fs::write(schema_dir.join("schema.json"), schema).unwrap();
}

fn create_values(dir: &Path, namespace: &str, values: &str) {
    let ns_dir = dir.join(namespace);
    fs::create_dir_all(&ns_dir).unwrap();
    fs::write(ns_dir.join("values.json"), values).unwrap();
}

fn setup() -> (Options, tempfile::TempDir) {
    let temp = tempfile::TempDir::new().unwrap();
    let schemas = temp.path().join("schemas");
    let values = temp.path().join("values");
    fs::create_dir_all(&schemas).unwrap();

    create_schema(
        &schemas,
        "bench",
        r#"{
            "version": "1.0",
            "type": "object",
            "properties": {
                "str-opt": {"type": "string", "default": "", "description": "string option"},
                "int-opt": {"type": "integer", "default": 0, "description": "integer option"},
                "bool-opt": {"type": "boolean", "default": false, "description": "boolean option"},
                "num-opt": {"type": "number", "default": 0.0, "description": "number option"},
                "obj-opt": {
                    "type": "object",
                    "properties": {"host": {"type": "string"}, "port": {"type": "integer"}},
                    "default": {"host": "localhost", "port": 8080},
                    "description": "object option"
                },
                "arr-opt": {
                    "type": "array",
                    "items": {"type": "integer"},
                    "default": [],
                    "description": "array option"
                },
                "default-only": {"type": "string", "default": "fallback", "description": "no value set"}
            }
        }"#,
    );

    create_values(
        &values,
        "bench",
        r#"{
            "options": {
                "str-opt": "hello world",
                "int-opt": 42,
                "bool-opt": true,
                "num-opt": 3.14,
                "obj-opt": {"host": "example.com", "port": 9090},
                "arr-opt": [1, 2, 3, 4, 5]
            }
        }"#,
    );

    let opts = Options::from_directory(temp.path()).unwrap();
    (opts, temp)
}

fn bench_get(c: &mut Criterion) {
    let (opts, _temp) = setup();
    let mut group = c.benchmark_group("get");

    group.bench_function("string", |b| {
        b.iter(|| black_box(opts.get("bench", "str-opt").unwrap()));
    });
    group.bench_function("integer", |b| {
        b.iter(|| black_box(opts.get("bench", "int-opt").unwrap()));
    });
    group.bench_function("boolean", |b| {
        b.iter(|| black_box(opts.get("bench", "bool-opt").unwrap()));
    });
    group.bench_function("number", |b| {
        b.iter(|| black_box(opts.get("bench", "num-opt").unwrap()));
    });
    group.bench_function("object", |b| {
        b.iter(|| black_box(opts.get("bench", "obj-opt").unwrap()));
    });
    group.bench_function("array", |b| {
        b.iter(|| black_box(opts.get("bench", "arr-opt").unwrap()));
    });
    group.bench_function("default_fallback", |b| {
        b.iter(|| black_box(opts.get("bench", "default-only").unwrap()));
    });

    group.finish();
}

criterion_group!(benches, bench_get);
criterion_main!(benches);
