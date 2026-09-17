use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sentry_options::{Assignment, AssignmentStatus, ExperimentChecker, ExperimentContext, Options};
use serde_json::json;
use tempfile::TempDir;

const NS: &str = "sentry-options-testing";
const BIN: &str = env!("CARGO_BIN_EXE_sentry-options-cli");

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("cli crate has a parent")
        .to_path_buf()
}

fn org_ctx(id: i64) -> ExperimentContext {
    HashMap::from([("organization_id".to_string(), json!(id))])
}

fn run_ctx(id: i64) -> ExperimentContext {
    HashMap::from([("run_id".to_string(), json!(id))])
}

fn seed_fixture(dir: &Path) {
    let src = repo_root().join("sentry-options");
    let schema_dst = dir.join("schemas").join(NS);
    let yaml_dst = dir.join("options").join(NS).join("default");
    fs::create_dir_all(&schema_dst).unwrap();
    fs::create_dir_all(&yaml_dst).unwrap();
    fs::copy(
        src.join("schemas").join(NS).join("schema.json"),
        schema_dst.join("schema.json"),
    )
    .unwrap();
    fs::copy(
        src.join("options")
            .join(NS)
            .join("default")
            .join("base.yaml"),
        yaml_dst.join("base.yaml"),
    )
    .unwrap();
}

fn validate_values(dir: &Path) -> std::process::Output {
    Command::new(BIN)
        .args([
            "validate-values",
            "--schemas",
            dir.join("schemas").to_str().unwrap(),
            "--root",
            dir.join("options").to_str().unwrap(),
        ])
        .output()
        .unwrap()
}

fn write_and_place(dir: &Path) {
    let out = dir.join("gen");
    let status = Command::new(BIN)
        .args([
            "write",
            "--schemas",
            dir.join("schemas").to_str().unwrap(),
            "--root",
            dir.join("options").to_str().unwrap(),
            "--output-format",
            "json",
            "--out",
            out.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success(), "write exited non-zero");

    let generated = fs::read_dir(&out)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.extension().is_some_and(|x| x == "json"))
        .expect("write produced a json file");
    let values_dir = dir.join("values").join(NS);
    fs::create_dir_all(&values_dir).unwrap();
    fs::copy(&generated, values_dir.join("values.json")).unwrap();
}

fn build_options(dir: &Path) -> &'static Options {
    Box::leak(Box::new(
        Options::builder()
            .with_directory(dir)
            .with_refresh_threshold(None)
            .build()
            .unwrap(),
    ))
}

#[track_caller]
fn assert_assignment(a: &Assignment, slot: u32, status: AssignmentStatus, arm: Option<&str>) {
    assert_eq!(a.slot, Some(slot), "slot");
    assert_eq!(a.status, status, "status");
    assert_eq!(a.arm.as_deref(), arm, "arm");
}

#[test]
fn yaml_to_json_to_client_assignments_and_reload() {
    let dir = TempDir::new().unwrap();
    seed_fixture(dir.path());

    assert!(
        validate_values(dir.path()).status.success(),
        "validate-values on the seed fixture should pass"
    );

    write_and_place(dir.path());
    let options = build_options(dir.path());
    let checker = ExperimentChecker::new(NS.to_string(), options);

    let color1 = checker.assign("checkout-color", &org_ctx(1));
    assert_assignment(&color1, 6, AssignmentStatus::Assigned, Some("control"));
    assert_eq!(color1.config, None);

    let color5 = checker.assign("checkout-color", &org_ctx(5));
    assert_assignment(&color5, 3, AssignmentStatus::Assigned, Some("treatment"));
    assert_eq!(color5.config, Some(json!({"color": "green"})));

    let color16 = checker.assign("checkout-color", &org_ctx(16));
    assert_assignment(&color16, 58, AssignmentStatus::Excluded, None);
    assert_eq!(color16.excluded_by.as_deref(), Some("checkout-copy"));

    let color4 = checker.assign("checkout-color", &org_ctx(4));
    assert_assignment(&color4, 74, AssignmentStatus::Holdout, None);

    let copy37 = checker.assign("checkout-copy", &org_ctx(37));
    assert_assignment(&copy37, 45, AssignmentStatus::Assigned, Some("short"));

    let paused = checker.assign("paused-experiment", &run_ctx(1));
    assert_assignment(&paused, 13, AssignmentStatus::Disabled, None);

    let victim = (1..200)
        .find(|&org| {
            let a = checker.assign("checkout-color", &org_ctx(org));
            a.status == AssignmentStatus::Assigned
                && matches!(a.slot, Some(s) if (30..=39).contains(&s))
        })
        .expect("an org with a checkout-color slot in 30..=39");

    let yaml = dir
        .path()
        .join("options")
        .join(NS)
        .join("default")
        .join("base.yaml");
    let shrunk = fs::read_to_string(&yaml)
        .unwrap()
        .replace("size: 40", "size: 30");
    fs::write(&yaml, shrunk).unwrap();
    write_and_place(dir.path());
    assert!(
        options.refresh().unwrap(),
        "reload should publish new values"
    );

    assert_eq!(
        checker.assign("checkout-color", &org_ctx(victim)).status,
        AssignmentStatus::Holdout,
        "victim slot 30..=39 falls outside the shrunk 0..30 allocation"
    );
    assert_assignment(
        &checker.assign("checkout-color", &org_ctx(1)),
        6,
        AssignmentStatus::Assigned,
        Some("control"),
    );
    assert_assignment(
        &checker.assign("checkout-color", &org_ctx(5)),
        3,
        AssignmentStatus::Assigned,
        Some("treatment"),
    );
}

fn seed_schema(dir: &Path) {
    let src = repo_root().join("sentry-options");
    let schema_dst = dir.join("schemas").join(NS);
    fs::create_dir_all(&schema_dst).unwrap();
    fs::copy(
        src.join("schemas").join(NS).join("schema.json"),
        schema_dst.join("schema.json"),
    )
    .unwrap();
}

fn write_yaml(dir: &Path, target: &str, name: &str, body: &str) {
    let dst = dir.join("options").join(NS).join(target);
    fs::create_dir_all(&dst).unwrap();
    fs::write(dst.join(name), body).unwrap();
}

fn write_values(dir: &Path) -> std::process::Output {
    let out = dir.join("gen");
    Command::new(BIN)
        .args([
            "write",
            "--schemas",
            dir.join("schemas").to_str().unwrap(),
            "--root",
            dir.join("options").to_str().unwrap(),
            "--output-format",
            "json",
            "--out",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap()
}

fn layer_yaml(layer: &str, unit: &str, experiment: &str, size: u32) -> String {
    format!(
        "options:\n  experiment-layer.{layer}:\n    unit: [{unit}]\n    experiments:\n      {experiment}:\n        owner: {{ team: testing }}\n        allocation: {{ start: 0, size: {size} }}\n        arms:\n          - {{ name: control, weight: 1 }}\n"
    )
}

#[test]
fn same_experiment_in_two_layers_same_target_fails() {
    let dir = TempDir::new().unwrap();
    seed_schema(dir.path());
    write_yaml(
        dir.path(),
        "default",
        "a.yaml",
        &layer_yaml("checkout", "organization_id", "shared", 10),
    );
    write_yaml(
        dir.path(),
        "default",
        "b.yaml",
        &layer_yaml("paused", "run_id", "shared", 10),
    );

    let output = validate_values(dir.path());
    assert!(!output.status.success(), "merged duplicate should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("target 'default'"), "stderr was: {stderr}");
    assert!(
        stderr.contains("declared in both experiment-layer.checkout and experiment-layer.paused"),
        "stderr was: {stderr}"
    );

    let write = write_values(dir.path());
    assert!(!write.status.success(), "write should fail too");
}

#[test]
fn same_experiment_across_default_and_target_fails_only_on_merge() {
    let dir = TempDir::new().unwrap();
    seed_schema(dir.path());
    write_yaml(
        dir.path(),
        "default",
        "base.yaml",
        &layer_yaml("checkout", "organization_id", "x", 10),
    );
    write_yaml(
        dir.path(),
        "de",
        "override.yaml",
        &layer_yaml("paused", "run_id", "x", 10),
    );

    let output = validate_values(dir.path());
    assert!(!output.status.success(), "merged de should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("target 'de'"), "stderr was: {stderr}");
    assert!(
        stderr.contains("declared in both experiment-layer.checkout and experiment-layer.paused"),
        "stderr was: {stderr}"
    );
}

#[test]
fn overriding_a_whole_layer_for_one_target_passes() {
    let dir = TempDir::new().unwrap();
    seed_schema(dir.path());
    write_yaml(
        dir.path(),
        "default",
        "base.yaml",
        &layer_yaml("checkout", "organization_id", "checkout-color", 40),
    );
    write_yaml(
        dir.path(),
        "de",
        "override.yaml",
        &layer_yaml("checkout", "organization_id", "checkout-color", 60),
    );

    let output = validate_values(dir.path());
    assert!(
        output.status.success(),
        "a whole-layer override replaces the key and is not a duplicate; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn overlapping_allocation_fails_validation() {
    let dir = TempDir::new().unwrap();
    seed_fixture(dir.path());

    let yaml = dir
        .path()
        .join("options")
        .join(NS)
        .join("default")
        .join("base.yaml");
    let overlapping = fs::read_to_string(&yaml)
        .unwrap()
        .replace("start: 40", "start: 20");
    fs::write(&yaml, overlapping).unwrap();

    let output = validate_values(dir.path());
    assert!(!output.status.success(), "overlap should fail validation");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("overlap"), "stderr was: {stderr}");
    assert!(stderr.contains("free slots:"), "stderr was: {stderr}");
}
