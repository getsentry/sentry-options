//! Native sidecar that keeps sentry-options ConfigMaps synced into shared
//! volumes, replacing kubelet's periodic ConfigMap volume refresh.
//!
//! The envoy-injector adds this container with `restartPolicy: Always` and a
//! startup probe running `sentry-options-sync ready`, so the app container
//! starts only after the first sync (or the startup timeout) completes.
//!
//! Environment:
//! - `SENTRY_OPTIONS_SYNC_CONFIGMAPS`: comma-separated `configmap=directory`
//!   pairs, e.g. `sentry-options-getsentry=/etc/sentry-options/values/getsentry`.
//! - `SENTRY_OPTIONS_SYNC_STARTUP_TIMEOUT_SECONDS`: how long to wait for the
//!   first sync before reporting ready anyway (default 30). ConfigMaps are
//!   optional, so the app then starts on schema defaults, as it would with a
//!   missing ConfigMap volume.
//! - `SENTRY_OPTIONS_SYNC_STATSD_ADDR`: optional DogStatsD `host:port`.
//!
//! Metrics:
//! - `sentry.options.sync.generation_to_write` (distribution, seconds, tagged
//!   `namespace` and `configmap`): from the values' `generated_at` to this pod
//!   writing them, for each live update. It covers the deploy pipeline and the
//!   watch. Subtracting it from the client's `propagation_delay`
//!   (`generated_at` to client refresh) leaves the client's refresh-on-read lag.

mod metrics;
mod writer;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::runtime::WatchStreamExt;
use kube::runtime::watcher::{self, Event};
use kube::{Api, Client};
use tokio::sync::mpsc;

use metrics::Metrics;

const READY_MARKER: &str = "/tmp/sentry-options-sync-ready";
const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

fn parse_mappings(spec: &str) -> Result<Vec<(String, PathBuf)>> {
    spec.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((name, dir)) if !name.is_empty() && !dir.is_empty() => {
                Ok((name.to_string(), PathBuf::from(dir)))
            }
            _ => bail!("invalid mapping {pair:?}, expected configmap=directory"),
        })
        .collect()
}

fn configmap_data(configmap: Option<&ConfigMap>) -> BTreeMap<String, String> {
    configmap.and_then(|c| c.data.clone()).unwrap_or_default()
}

/// Seconds from the payload's `values.json` `generated_at` until `now`.
fn generation_delay(data: &BTreeMap<String, String>, now: DateTime<Utc>) -> Option<f64> {
    let values: serde_json::Value = serde_json::from_str(data.get("values.json")?).ok()?;
    let generated_at = DateTime::parse_from_rfc3339(values.get("generated_at")?.as_str()?).ok()?;
    Some(
        (now - generated_at.with_timezone(&Utc))
            .as_seconds_f64()
            .max(0.0),
    )
}

/// Watch one ConfigMap by name and project every change into `dir`.
async fn sync(
    api: Api<ConfigMap>,
    name: String,
    dir: PathBuf,
    synced: mpsc::Sender<()>,
    metrics: Arc<Metrics>,
) {
    let namespace = dir
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let config = watcher::Config::default().fields(&format!("metadata.name={name}"));
    let mut events = std::pin::pin!(watcher::watcher(api, config).default_backoff());
    let mut written: Option<BTreeMap<String, String>> = None;
    let mut listed: Option<ConfigMap> = None;

    loop {
        // Only live updates are measured. The initial list reflects values
        // generated before this pod started, not a deploy reaching it.
        let mut live = false;
        let data = match events.try_next().await {
            Ok(Some(Event::Init)) => {
                listed = None;
                continue;
            }
            Ok(Some(Event::InitApply(configmap))) => {
                listed = Some(configmap);
                continue;
            }
            Ok(Some(Event::InitDone)) => configmap_data(listed.take().as_ref()),
            Ok(Some(Event::Apply(configmap))) => {
                live = true;
                configmap_data(Some(&configmap))
            }
            Ok(Some(Event::Delete(_))) => BTreeMap::new(),
            Ok(None) => return,
            Err(err) => {
                eprintln!("sentry-options-sync: watch {name}: {err}");
                continue;
            }
        };
        // Relists after a reconnect replay unchanged data; skip them so the
        // client does not see a new mtime without a new value.
        if written.as_ref() != Some(&data) {
            match writer::write_atomic(&dir, &data) {
                Ok(()) => {
                    let delay = generation_delay(&data, Utc::now()).filter(|_| live);
                    if let Some(delay) = delay {
                        metrics.distribution(
                            "sentry.options.sync.generation_to_write",
                            delay,
                            &[("namespace", &namespace), ("configmap", &name)],
                        );
                    }
                    eprintln!(
                        "sentry-options-sync: wrote {name} to {}{}",
                        dir.display(),
                        delay
                            .map_or_else(String::new, |d| format!(" ({d:.3}s after generated_at)")),
                    );
                    written = Some(data);
                }
                Err(err) => {
                    eprintln!("sentry-options-sync: write {name}: {err:#}");
                    continue;
                }
            }
        }
        let _ = synced.try_send(());
    }
}

async fn run() -> Result<()> {
    let mappings = parse_mappings(
        &std::env::var("SENTRY_OPTIONS_SYNC_CONFIGMAPS")
            .context("SENTRY_OPTIONS_SYNC_CONFIGMAPS is not set")?,
    )?;
    let startup_timeout = match std::env::var("SENTRY_OPTIONS_SYNC_STARTUP_TIMEOUT_SECONDS") {
        Ok(v) => Duration::from_secs(v.parse().context("invalid startup timeout")?),
        Err(_) => DEFAULT_STARTUP_TIMEOUT,
    };

    let metrics = Arc::new(Metrics::new(
        std::env::var("SENTRY_OPTIONS_SYNC_STATSD_ADDR")
            .ok()
            .as_deref()
            .filter(|s| !s.is_empty()),
    ));

    let client = Client::try_default().await?;
    let api: Api<ConfigMap> = Api::default_namespaced(client);

    let mut first_syncs = Vec::new();
    for (name, dir) in mappings {
        let (tx, rx) = mpsc::channel(1);
        first_syncs.push(rx);
        tokio::spawn(sync(api.clone(), name, dir, tx, metrics.clone()));
    }

    let all_synced = async {
        for rx in &mut first_syncs {
            rx.recv().await;
        }
    };
    if tokio::time::timeout(startup_timeout, all_synced)
        .await
        .is_err()
    {
        eprintln!(
            "sentry-options-sync: first sync incomplete after {startup_timeout:?}; \
             reporting ready so the app starts on defaults"
        );
    }
    std::fs::write(READY_MARKER, b"")?;
    std::future::pending().await
}

fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() == Some("ready") {
        std::fs::metadata(READY_MARKER).context("first sync has not completed")?;
        return Ok(());
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(run())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mappings() {
        assert_eq!(
            parse_mappings(" a=/x , b=/y,").unwrap(),
            vec![
                ("a".to_string(), PathBuf::from("/x")),
                ("b".to_string(), PathBuf::from("/y")),
            ]
        );
        assert!(parse_mappings("a").is_err());
        assert!(parse_mappings("=/x").is_err());
    }

    #[test]
    fn measures_generation_delay() {
        let now = DateTime::parse_from_rfc3339("2026-01-01T00:00:02.5Z")
            .unwrap()
            .with_timezone(&Utc);
        let data = |values: &str| BTreeMap::from([("values.json".to_string(), values.to_string())]);

        assert_eq!(
            generation_delay(
                &data(r#"{"options":{},"generated_at":"2026-01-01T00:00:00Z"}"#),
                now
            ),
            Some(2.5)
        );
        assert_eq!(generation_delay(&data(r#"{"options":{}}"#), now), None);
        assert_eq!(generation_delay(&data("not json"), now), None);
        assert_eq!(generation_delay(&BTreeMap::new(), now), None);
    }
}
