use std::fs;
use std::process::{Command, Output};

use tempfile::TempDir;

const VALUES: &str = r#"
options:
  feature.organizations:test:
    created_at: "2026-01-01"
    enabled: true
    owner:
      team: dev-infra
    segments:
      - name: internal
        rollout: 100
        conditions:
          - property: organization_slug
            operator: in
            value: [sentry]
      - name: nobody
        rollout: 0
        conditions:
          - property: organization_slug
            operator: in
            value: [acme]
  feature.organizations:off:
    created_at: "2026-01-01"
    enabled: false
    owner:
      team: dev-infra
    segments:
      - name: everyone
        rollout: 100
        conditions: []
"#;

fn eval(flag: &str, context: &str) -> Output {
    let dir = TempDir::new().unwrap();
    let values = dir.path().join("flagpole.yaml");
    fs::write(&values, VALUES).unwrap();
    Command::new(env!("CARGO_BIN_EXE_sentry-options-cli"))
        .args(["eval", "--values"])
        .arg(&values)
        .args(["--flag", flag, "--context", context])
        .output()
        .unwrap()
}

fn stdout(flag: &str, context: &str) -> String {
    let out = eval(flag, context);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn stderr(flag: &str, context: &str) -> String {
    let out = eval(flag, context);
    assert!(!out.status.success());
    String::from_utf8(out.stderr).unwrap()
}

#[test]
fn test_reports_matching_segment() {
    assert_eq!(
        stdout("organizations:test", r#"{"organization_slug": "sentry"}"#),
        "true\nsegment: internal (rollout 100%)\n"
    );
}

#[test]
fn test_accepts_feature_prefix() {
    assert!(
        stdout(
            "feature.organizations:test",
            r#"{"organization_slug": "sentry"}"#
        )
        .starts_with("true\n")
    );
}

#[test]
fn test_reports_matched_segment_outside_rollout() {
    assert_eq!(
        stdout("organizations:test", r#"{"organization_slug": "acme"}"#),
        "false\nsegment: nobody (rollout 0%)\nconditions matched, but the context is outside the rollout\n"
    );
}

#[test]
fn test_reports_no_match() {
    assert_eq!(
        stdout("organizations:test", r#"{"organization_slug": "other"}"#),
        "false\nno segment matched\n"
    );
}

#[test]
fn test_reports_disabled() {
    assert_eq!(
        stdout("organizations:off", "{}"),
        "false\nflag is disabled\n"
    );
}

#[test]
fn test_missing_flag_errors() {
    assert!(stderr("organizations:nope", "{}").contains("feature.organizations:nope not found"));
}

#[test]
fn test_invalid_context_errors() {
    assert!(stderr("organizations:test", "not json").contains("not valid JSON"));
    assert!(stderr("organizations:test", "[1]").contains("must be a JSON object"));
}
