use std::fs;

use clap::Args;
use sentry_options::{FeatureContext, Options, feature_property, features};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::{AppError, Result};

const NAMESPACE: &str = "eval";

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
    let segments = definition
        .get("segments")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Validation(format!("{key} has no segments list")))?;

    // Each segment gets its own probe flag at 100% rollout, so a probe is true
    // exactly when that segment's conditions match.
    let mut flags = vec![(name.to_string(), definition.clone())];
    for (i, segment) in segments.iter().enumerate() {
        let mut segment = segment.clone();
        // Non-object segments are left for the schema check to reject.
        if let Some(fields) = segment.as_object_mut() {
            fields.insert("rollout".to_string(), json!(100));
        }
        let mut probe = definition.clone();
        probe["enabled"] = json!(true);
        probe["segments"] = json!([segment]);
        flags.push((format!("eval-segment-{i}"), probe));
    }

    let _dir = init_options(&flags)?;
    let checker = features(NAMESPACE);
    let context = build_context(&args.context, &args.identity_fields)?;
    let has = |flag: &str| checker.try_has(flag, &context).map(|r| r == Some(true));

    let result = has(name).map_err(|e| AppError::Validation(format!("{key}: {e}")))?;
    let mut out = format!("{result}\n");
    if definition.get("enabled") == Some(&Value::Bool(false)) {
        out += "flag is disabled\n";
        return Ok(out);
    }
    let mut matched = None;
    for i in 0..segments.len() {
        if has(&format!("eval-segment-{i}")).unwrap_or(false) {
            matched = Some(i);
            break;
        }
    }
    match matched {
        Some(i) => {
            let segment_name = segments[i]["name"].as_str().unwrap_or("(unnamed)");
            let rollout = segments[i]["rollout"].as_u64().unwrap_or(100);
            out += &format!("segment: {segment_name} (rollout {rollout}%)\n");
            if !result {
                out += "conditions matched, but the context is outside the rollout\n";
            }
        }
        None => out += "no segment matched\n",
    }
    Ok(out)
}

/// Initializes the global options store with the given `(name, definition)`
/// flags, from a throwaway values directory that must outlive the evaluation.
fn init_options(flags: &[(String, Value)]) -> Result<TempDir> {
    let schema = json!({
        "version": "1.0",
        "type": "object",
        "patternProperties": { "^feature\\.": feature_property() },
    })
    .to_string();
    let values: serde_json::Map<String, Value> = flags
        .iter()
        .map(|(name, definition)| (format!("feature.{name}"), definition.clone()))
        .collect();

    let dir = TempDir::new()?;
    let values_dir = dir.path().join("values").join(NAMESPACE);
    fs::create_dir_all(&values_dir)?;
    fs::write(
        values_dir.join("values.json"),
        json!({ "options": values }).to_string(),
    )?;

    Options::builder()
        .with_directory(dir.path())
        .with_schemas(&[(NAMESPACE, &schema)])
        .init()
        .map_err(|e| AppError::Validation(format!("Invalid flag definition: {e}")))?;
    Ok(dir)
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
