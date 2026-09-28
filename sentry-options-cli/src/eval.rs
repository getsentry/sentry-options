use std::fs;

use clap::Args;
use sentry_options::{FeatureContext, evaluate_value};
use serde_json::Value;

use crate::{AppError, Result};

#[derive(Args, Debug)]
pub struct EvalArgs {
    #[arg(
        long,
        help = "flagpole.yaml or another values file with a top level 'options' mapping"
    )]
    values: String,

    #[arg(long, help = "flag name, with or without the 'feature.' prefix")]
    flag: String,

    #[arg(
        long,
        default_value = "{}",
        help = "evaluation context as a JSON object"
    )]
    context: String,

    #[arg(
        long,
        value_delimiter = ',',
        default_value = "organization_id,project_id",
        help = "comma separated context fields used for rollout bucketing"
    )]
    identity_fields: Vec<String>,
}

pub fn cli_eval(args: EvalArgs) -> Result<()> {
    print!("{}", eval(&args)?);
    Ok(())
}

fn eval(args: &EvalArgs) -> Result<String> {
    let content = fs::read_to_string(&args.values)
        .map_err(|e| AppError::Validation(format!("Cannot read {}: {e}", args.values)))?;
    let file: Value = serde_yaml::from_str(&content).map_err(|e| AppError::YamlParse {
        path: args.values.clone(),
        source: e,
    })?;

    let name = args.flag.strip_prefix("feature.").unwrap_or(&args.flag);
    let key = format!("feature.{name}");
    let definition = file
        .get("options")
        .and_then(|options| options.get(&key))
        .ok_or_else(|| AppError::Validation(format!("{key} not found in {}", args.values)))?;

    let context = build_context(&args.context, &args.identity_fields)?;
    let evaluation = evaluate_value(definition, &context)
        .ok_or_else(|| AppError::Validation(format!("{key} is not a valid feature")))?;

    let mut out = format!("{}\n", evaluation.result);
    match evaluation.segment {
        Some(index) => {
            let segment = &definition["segments"][index];
            let segment_name = segment["name"].as_str().unwrap_or("(unnamed)");
            let rollout = segment["rollout"].as_u64().unwrap_or(100);
            out += &format!("segment: {segment_name} (rollout {rollout}%)\n");
            if !evaluation.result {
                out += "conditions matched, but the context is outside the rollout\n";
            }
        }
        None if definition.get("enabled") == Some(&Value::Bool(false)) => {
            out += "flag is disabled\n";
        }
        None => out += "no segment matched\n",
    }
    Ok(out)
}

fn build_context(context: &str, identity_fields: &[String]) -> Result<FeatureContext> {
    let Value::Object(data) = serde_json::from_str(context)
        .map_err(|e| AppError::Validation(format!("--context is not valid JSON: {e}")))?
    else {
        return Err(AppError::Validation(format!(
            "--context must be a JSON object, got {context}"
        )));
    };
    let mut ctx = FeatureContext::new();
    for (key, value) in data {
        ctx.insert(&key, value);
    }
    ctx.identity_fields(identity_fields.iter().map(String::as_str).collect());
    Ok(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

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

    fn run(flag: &str, context: &str) -> Result<String> {
        let file = NamedTempFile::new().unwrap();
        fs::write(file.path(), VALUES).unwrap();
        eval(&EvalArgs {
            values: file.path().display().to_string(),
            flag: flag.to_string(),
            context: context.to_string(),
            identity_fields: vec!["organization_id".into(), "project_id".into()],
        })
    }

    #[test]
    fn test_reports_matching_segment() {
        let out = run("organizations:test", r#"{"organization_slug": "sentry"}"#).unwrap();
        assert_eq!(out, "true\nsegment: internal (rollout 100%)\n");
    }

    #[test]
    fn test_accepts_feature_prefix() {
        let out = run(
            "feature.organizations:test",
            r#"{"organization_slug": "sentry"}"#,
        )
        .unwrap();
        assert!(out.starts_with("true\n"));
    }

    #[test]
    fn test_reports_matched_segment_outside_rollout() {
        let out = run("organizations:test", r#"{"organization_slug": "acme"}"#).unwrap();
        assert_eq!(
            out,
            "false\nsegment: nobody (rollout 0%)\nconditions matched, but the context is outside the rollout\n"
        );
    }

    #[test]
    fn test_reports_no_match() {
        let out = run("organizations:test", r#"{"organization_slug": "other"}"#).unwrap();
        assert_eq!(out, "false\nno segment matched\n");
    }

    #[test]
    fn test_reports_disabled() {
        let out = run("organizations:off", "{}").unwrap();
        assert_eq!(out, "false\nflag is disabled\n");
    }

    #[test]
    fn test_missing_flag_errors() {
        let err = run("organizations:nope", "{}").unwrap_err();
        assert!(
            err.to_string()
                .contains("feature.organizations:nope not found")
        );
    }

    #[test]
    fn test_invalid_context_errors() {
        assert!(run("organizations:test", "not json").is_err());
        let err = run("organizations:test", "[1]").unwrap_err();
        assert!(err.to_string().contains("must be a JSON object"));
    }
}
