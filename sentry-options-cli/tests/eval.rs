use std::fs;
use std::process::Command;

use tempfile::TempDir;

const VALUES: &str = r#"
options:
  feature.organizations:test:
    created_at: "2026-01-01"
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

fn eval(flag: &str, context: &str) -> String {
    let dir = TempDir::new().unwrap();
    let values = dir.path().join("flagpole.yaml");
    fs::write(&values, VALUES).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_sentry-options-cli"))
        .args(["eval", "--values"])
        .arg(&values)
        .args(["--flag", flag, "--context", context])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn test_eval_reports_deciding_segment() {
    let slug = |s: &str| format!(r#"{{"organization_slug": "{s}"}}"#);
    assert_eq!(
        eval("organizations:test", &slug("sentry")),
        "true\nsegment: internal (rollout 100%)\n"
    );
    assert_eq!(
        eval("organizations:test", &slug("acme")),
        "false\nsegment: nobody (rollout 0%)\nconditions matched, but the context is outside the rollout\n"
    );
    assert_eq!(
        eval("organizations:test", &slug("other")),
        "false\nno segment matched\n"
    );
    assert_eq!(eval("organizations:off", "{}"), "false\nflag is disabled\n");
}
