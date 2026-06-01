# Complete Option Classification

Every `register()` call in sentry and getsentry, classified into a destination.

> Generated from `inventory.csv` on the `kjiang/options-inventory` branch.
> Inventory run against latest sentry/getsentry master as of 2026-06-01.
> Excludes runtime-only options (flagpole feature flags, safe rollouts) which are
> not found via static analysis without `active_options.json`.
> Total: **738 options**.

## Summary

| Destination | Count | Description |
|-------------|-------|-------------|
| **Move to new sentry-options** | 656 | All have `FLAG_AUTOMATOR_MODIFIABLE` — already managed by the automator |
| **Move to Django settings** | 48 | Per-environment config: `FLAG_NOSTORE`, `FLAG_REQUIRED`, `FLAG_PRIORITIZE_DISK` |
| **Delete** | 5 | Unused (`SAFE_CANDIDATE`/`INVESTIGATE`) |
| **Credentials (secrets mgmt)** | 29 | Options with `FLAG_CREDENTIAL` — secrets that should not be in ConfigMaps |
| **Total** | **738** | |

> **Not included:** ~288 flagpole feature flags and ~24 other runtime-registered
> options that only appear when booting a live instance. These are covered by
> a separate migration phase (Step 2.2 in the plan).

---

## Move to new sentry-options (656 options)

These options all have `FLAG_AUTOMATOR_MODIFIABLE` and are already managed by the
automator pipeline today. They need to be defined in a sentry-options schema, have
their values set in sentry-options-automator's new `option-values/` directory, and
be read by the new `get()` fallback mechanism.

### FLAG_AUTOMATOR_MODIFIABLE (655)

| Option | Type | Default | Usages | DISK |
|--------|------|---------|--------|------|
| `api-token-async-flush` | Bool | `False` | 2 |  |
| `api.deprecation.brownout-cron` | String | `"0 12 * * *"` | 2 |  |
| `api.deprecation.brownout-duration` | Int | `60` | 2 |  |
| `api.organization.disable-last-deploys` | Sequence | `[]` | 3 |  |
| `api.project-transfer.rate-limit-overrides` | Int | `3` | 2 |  |
| `api.rate-limit.org-create` | inferred | `5` | 5 | DISK |
| `apigateway.cell_resolver.enabled` | Bool | `False` | 4 |  |
| `apigateway.proxy.circuit-breaker.config` | Dict | `{         "error_limit": 100` | 1 |  |
| `apigateway.proxy.circuit-breaker.enabled` | Bool | `False` | 1 |  |
| `apigateway.proxy.circuit-breaker.enforce` | Bool | `False` | 1 |  |
| `apigateway.proxy.timeout` | Int | `None` | 1 |  |
| `auth.ip-rate-limit` | inferred | `0` | 4 | DISK |
| `auth.user-rate-limit` | inferred | `0` | 4 | DISK |
| `aws-lambda.access-key-id` | inferred | `` | 3 | DISK |
| `aws-lambda.account-number` | inferred | `"943013980633"` | 4 |  |
| `aws-lambda.cloudformation-url` | inferred | `` | 4 |  |
| `aws-lambda.host-region` | inferred | `"us-east-2"` | 1 |  |
| `aws-lambda.node.layer-name` | inferred | `"SentryNodeServerlessSDK"` | 1 |  |
| `aws-lambda.node.layer-version` | inferred | `` | 1 |  |
| `aws-lambda.python.layer-name` | inferred | `"SentryPythonServerlessSDK"` | 1 |  |
| `aws-lambda.python.layer-version` | inferred | `` | 1 |  |
| `aws-lambda.thread-count` | inferred | `100` | 1 |  |
| `backfill_new_categories.categories` | Sequence | `[]` | 3 |  |
| `backfill_new_categories.chunk_size` | Int | `100` | 2 |  |
| `backfill_new_categories.lock_ttl` | Int | `30 * 60` | 1 |  |
| `backfill_new_categories.org_ids` | Sequence | `[]` | 3 |  |
| `backfill_new_categories.prioritize_paid_plans` | Bool | `True` | 1 |  |
| `backfill_new_categories.should_run` | Bool | `False` | 2 |  |
| `backpressure.checking.enabled` | inferred | `False` | 3 |  |
| `backpressure.checking.interval` | inferred | `5` | 2 |  |
| `backpressure.high_watermarks.attachments-store` | inferred | `0.8` | 0 |  |
| `backpressure.high_watermarks.post-process-locks` | inferred | `0.8` | 0 |  |
| `backpressure.high_watermarks.processing-locks` | inferred | `0.8` | 0 |  |
| `backpressure.high_watermarks.processing-store` | inferred | `0.8` | 0 |  |
| `backpressure.high_watermarks.processing-store-transactions` | inferred | `0.8` | 0 |  |
| `backpressure.monitoring.enabled` | inferred | `False` | 4 |  |
| `backpressure.monitoring.interval` | inferred | `5` | 1 |  |
| `backpressure.status_ttl` | inferred | `60` | 3 |  |
| `billing.add-billing-metric-usage-admin.organizations` | Sequence | `[]` | 1 |  |
| `billing.create_invoices.tasks_per_second` | inferred | `24` | 3 |  |
| `billing.seat-based-seer-launch` | inferred | `False` | 3 |  |
| `billing.usage_service.cutover_date` | inferred | `""` | 2 |  |
| `billing.usage_service.enabled` | inferred | `False` | 2 |  |
| `billing.usagebuffer.batch_incr.projects` | Sequence | `[]` | 2 |  |
| `billing.usagebuffer.batch_incr.rollout` | inferred | `0.0` | 2 |  |
| `billing.usagebuffer.redis.pipeline_size` | inferred | `1000` | 2 |  |
| `billing.usagebuffer.scan_limit` | inferred | `10000` | 1 |  |
| `billing.usagebuffer.unified_pipeline.chunk_size` | inferred | `10000` | 2 |  |
| `billing.usagebuffer.unified_pipeline.rollout` | inferred | `0.0` | 3 |  |
| `chart-rendering.chartcuterie` | inferred | `{"url": "http://127.0.0.1:7901` | 4 | DISK |
| `chart-rendering.enabled` | inferred | `False` | 5 | DISK |
| `chart-rendering.storage.backend` | inferred | `None` | 3 | DISK |
| `chart-rendering.storage.options` | Dict | `None` | 3 | DISK |
| `chunk-upload.no-compression` | inferred | `[]` | 2 |  |
| `cleanup.abort_execution` | Bool | `False` | 1 |  |
| `consumer.dump_stacktrace_on_shutdown` | Sequence | `[]` | 1 |  |
| `consumer.join.profiling.rate` | Float | `0.0` | 1 |  |
| `consumer.shared_memory_spawn_process` | Bool | `False` | 1 |  |
| `consumer.verbose_multiprocessing_logs` | Sequence | `[]` | 1 |  |
| `continuous-profiling-beta` | inferred | `False` | 5 |  |
| `crons.dispatch_incident_occurrences_to_consumer` | inferred | `False` | 2 |  |
| `crons.organization.disable-check-in` | Sequence | `[]` | 3 |  |
| `crons.per_monitor_rate_limit` | Int | `6` | 4 | DISK |
| `crons.system_incidents.collect_metrics` | inferred | `False` | 3 |  |
| `crons.system_incidents.pct_deviation_anomaly_threshold` | inferred | `-10` | 2 |  |
| `crons.system_incidents.pct_deviation_incident_threshold` | inferred | `-30` | 2 |  |
| `crons.system_incidents.tick_decision_window` | inferred | `5` | 2 |  |
| `crons.system_incidents.use_decisions` | inferred | `False` | 4 |  |
| `dashboards.prebuilt-dashboard-ids` | Sequence | `[]` | 2 |  |
| `data-forwarding.project-cache-ttl` | Int | `300` | 1 |  |
| `data-forwarding.task-rollout-rate` | Float | `0.0` | 6 |  |
| `database.encryption.method` | String | `"plaintext"` | 5 | DISK |
| `delayed_processing.batch_size` | inferred | `10000` | 3 |  |
| `delayed_workflow.rollout` | Bool | `False` | 3 |  |
| `deletions.group-hash-metadata.batch-size` | Int | `1000` | 2 |  |
| `deletions.group-hashes-batch-size` | Int | `100` | 1 |  |
| `demo-mode.enabled` | Bool | `False` | 14 |  |
| `demo-mode.orgs` | inferred | `[]` | 8 |  |
| `demo-mode.users` | inferred | `[]` | 12 |  |
| `demo-org-ids` | inferred | `[]` | 1 |  |
| `devtoolbar.analytics.enabled` | Bool | `False` | 2 | DISK |
| `discord.application-id` | inferred | `` | 7 | DISK |
| `discord.debug-channel` | inferred | `` | 1 |  |
| `discord.debug-server` | inferred | `` | 1 |  |
| `discord.public-key` | inferred | `` | 6 | DISK |
| `dsym.cache-path` | String | `"/tmp/sentry-dsym-cache"` | 4 | DISK |
| `dynamic-sampling.check_span_feature_flag` | inferred | `False` | 6 |  |
| `dynamic-sampling.config.killswitch` | inferred | `False` | 1 |  |
| `dynamic-sampling.measure.spans` | Sequence | `[]` | 6 |  |
| `dynamic-sampling.per_org.killswitch` | inferred | `False` | 3 |  |
| `dynamic-sampling.per_org.metrics-sample-rate` | Float | `1.0` | 2 |  |
| `dynamic-sampling.per_org.rollout-rate` | Float | `0.0` | 3 |  |
| `dynamic-sampling.prioritise_transactions.num_explicit_large_transactions` | inferred | `` | 2 |  |
| `dynamic-sampling.prioritise_transactions.num_explicit_small_transactions` | inferred | `` | 2 |  |
| `dynamic-sampling.prioritise_transactions.rebalance_intensity` | inferred | `0.8` | 3 |  |
| `dynamic-sampling:sliding_window.size` | inferred | `24` | 1 |  |
| `eap-migration.alerts-transactions-rollback.queries` | inferred | `[]` | 2 |  |
| `eap-migration.alerts-transactions-rollforward.queries` | inferred | `[]` | 2 |  |
| `eap-migration.alerts-transactions.queries` | inferred | `[]` | 2 |  |
| `eap-migration.dashboard-comparison.enable` | inferred | `False` | 2 |  |
| `eap-migration.dashboard-comparison.projects` | inferred | `[]` | 2 |  |
| `eap-migration.dashboard-comparison.widget-queries` | inferred | `[]` | 2 |  |
| `eap-migration.dashboards-transactions.dashboards` | inferred | `[]` | 1 |  |
| `eap-migration.dashboards-transactions.organizations` | inferred | `[]` | 3 |  |
| `eap-migration.discover-transactions.enable` | inferred | `False` | 1 |  |
| `eap-migration.discover-transactions.organizations` | inferred | `[]` | 2 |  |
| `eap-migration.discover-transactions.projects` | inferred | `[]` | 2 |  |
| `eap-migration.discover-transactions.queries` | inferred | `[]` | 2 |  |
| `embeddings-grouping.seer.delete-record-batch-size` | Int | `100` | 2 |  |
| `eventstore.adjacent_event_ids_use_snql` | Bool | `False` | 1 |  |
| `eventstream.eap.deletion-enabled` | Bool | `True` | 2 |  |
| `eventstream.eap_forwarding_rate` | inferred | `0.0` | 3 |  |
| `eventstream:kafka-headers` | inferred | `True` | 1 |  |
| `explore.trace-items.keys.max` | Int | `1000` | 5 |  |
| `explore.trace-items.values.max` | Int | `1000` | 2 |  |
| `explorer.context_engine_indexing.enable` | Bool | `False` | 4 |  |
| `explorer.service_map.max_edges` | Int | `5000` | 2 |  |
| `explorer.service_map.max_segments` | Int | `500` | 1 | DISK |
| `explorer.service_map.parent_span_batch_size` | Int | `500` | 1 | DISK |
| `features.error.capture_rate` | inferred | `0.1` | 1 |  |
| `feedback.filter_garbage_messages` | Bool | `False` | 2 | DISK |
| `feedback.message.max-size` | Int | `4096` | 3 |  |
| `feedback.organizations.slug-denylist` | Sequence | `[]` | 2 |  |
| `filestore-timeout-seconds` | inferred | `5` | 1 |  |
| `filestore.migration.rollout` | inferred | `0.0` | 1 |  |
| `flagpole.allowed_features` | Sequence | `["*"]` | 4 |  |
| `flagpole.log_features` | inferred | `[]` | 1 |  |
| `flagpole.missing_features_logging_rate` | inferred | `0.1` | 1 |  |
| `flags:options-audit-log-is-enabled` | Bool | `True` | 2 | DISK |
| `flags:options-audit-log-organization-id` | Int | `None` | 2 | DISK |
| `getsentry.detect-low-value-spans.enabled` | Bool | `False` | 2 |  |
| `getsentry.detect-low-value-spans.internal-org-slugs` | Sequence | `[         "codecov"` | 2 |  |
| `getsentry.detect-low-value-spans.llm-max-tokens` | Int | `2048` | 1 |  |
| `getsentry.detect-low-value-spans.llm-processing-timeout` | Int | `20` | 1 |  |
| `getsentry.detect-low-value-spans.llm-reasoning` | String | `None` | 1 |  |
| `getsentry.detect-low-value-spans.llm-request-timeout` | Int | `60` | 1 |  |
| `getsentry.instance-sample-rate-per-project` | Dict | `{}` | 1 |  |
| `getsentry.options-dual-read-test` | Bool | `False` | 1 | DISK |
| `getsentry.override-sample-rate-for-complete-instance` | inferred | `False` | 1 |  |
| `getsentry.override-sample-rate-for-instance` | inferred | `False` | 1 |  |
| `getsentry.quotas.run_spike_projection.on_missing` | inferred | `False` | 2 |  |
| `getsentry.rate-limit.org-errors` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.org-feedback` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.org-logs` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.org-metric.seconds` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.org-profile.duration` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.org-profile.duration-ui` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.org-profiles` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.org-replays` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.org-spans` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.org-trace-metrics` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.org-transactions` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.project-errors` | Int | `0` | 3 | DISK |
| `getsentry.rate-limit.project-feedback` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.project-feedback-sustained.limit` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.project-feedback-sustained.window` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.project-logs` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.project-metric.seconds` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.project-profile.duration` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.project-profile.duration-ui` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.project-profiles` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.project-replays` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.project-spans` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.project-trace-metrics` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.project-transactions` | Int | `0` | 1 | DISK |
| `getsentry.rate-limit.window` | Int | `10` | 2 | DISK |
| `getsentry.spike-protection.calculate_spike_projections` | inferred | `True` | 2 |  |
| `github-app.client-id` | inferred | `` | 12 | DISK |
| `github-app.fetch-commits.max-compare-commits` | Int | `500` | 2 |  |
| `github-app.id` | inferred | `0` | 11 |  |
| `github-app.name` | inferred | `""` | 7 |  |
| `github-app.rate-limit-sensitive-orgs` | Sequence | `[]` | 2 |  |
| `github-console-sdk-app.client-id` | inferred | `""` | 1 |  |
| `github-console-sdk-app.id` | inferred | `0` | 6 |  |
| `github-enterprise-app.allowed-hosts-legacy-webhooks` | Sequence | `[]` | 2 |  |
| `github-login.api-domain` | inferred | `"api.github.com"` | 1 | DISK |
| `github-login.base-domain` | inferred | `"github.com"` | 1 | DISK |
| `github-login.client-id` | inferred | `""` | 7 | DISK |
| `github-login.extended-permissions` | Sequence | `[]` | 2 | DISK |
| `github-login.organization` | inferred | `` | 1 | DISK |
| `github-login.require-verified-email` | Bool | `False` | 1 | DISK |
| `github-secret-scanning.enable-signature-verification` | Bool | `True` | 2 |  |
| `github.webhook.mailbox-bucketing.enabled` | inferred | `False` | 2 |  |
| `grouping.config_transition.config_upgrade_sample_rate` | Float | `1.0` | 2 |  |
| `grouping.config_transition.metrics_sample_rate` | Float | `1.0` | 2 |  |
| `grouping.experimental_parameterization` | Float | `0.0` | 1 |  |
| `grouping.grouphash_metadata.ingestion_writes_enabled` | Bool | `True` | 2 |  |
| `grouping.merge.remove_stuck_group_redirects` | inferred | `False` | 1 |  |
| `grouping.merge.stuck_group_ids` | inferred | `[]` | 1 |  |
| `grouping.use_ingest_grouphash_caching` | Bool | `True` | 5 |  |
| `groups.enable-post-update-signal` | inferred | `False` | 9 |  |
| `hybrid_cloud.audit_log_event_id_invalid_pass_list` | Sequence | `[]` | 2 |  |
| `hybrid_cloud.authentication.disabled_organization_shards` | Sequence | `[]` | 3 |  |
| `hybrid_cloud.authentication.disabled_user_shards` | Sequence | `[]` | 3 |  |
| `hybrid_cloud.disable_relative_upload_urls` | inferred | `False` | 2 |  |
| `hybrid_cloud.disable_tombstone_cleanup` | inferred | `False` | 1 |  |
| `hybrid_cloud.rpc.disabled-service-methods` | inferred | `[]` | 2 |  |
| `hybridcloud.apigateway.use_pooling.rate` | Float | `0.0` | 2 |  |
| `hybridcloud.integrationproxy.retries` | inferred | `5` | 1 |  |
| `hybridcloud.regionsiloclient.retries` | inferred | `5` | 1 |  |
| `hybridcloud.rpc.method_retry_overrides` | inferred | `{}` | 2 |  |
| `hybridcloud.rpc.method_timeout_overrides` | inferred | `{}` | 3 |  |
| `hybridcloud.rpc.retries` | inferred | `5` | 2 |  |
| `hybridcloud.webhookpayload.push_drain_trigger` | inferred | `False` | 2 |  |
| `hybridcloud.webhookpayload.skip_on_failure_providers` | Sequence | `["github"]` | 1 |  |
| `hybridcloud.webhookpayload.worker_threads` | inferred | `4` | 2 |  |
| `inc-984.end` | inferred | `"2024-12-12"` | 2 |  |
| `inc-984.parallel.nodestore.read` | inferred | `8` | 1 |  |
| `inc-984.parallel.nodestore.threads` | inferred | `1` | 1 |  |
| `inc-984.projects` | inferred | `[]` | 3 |  |
| `inc-984.snuba.batches` | inferred | `1` | 2 |  |
| `inc-984.start` | inferred | `"2024-10-16"` | 2 |  |
| `indexed-spans-extraction-enabled` | inferred | `False` | 2 |  |
| `indexed-spans-extraction-orgs-denylist` | Sequence | `[]` | 2 |  |
| `insights-query-date-range-limit.enable` | Bool | `False` | 2 |  |
| `insights.span-samples-query.sample-rate` | Float | `0.0` | 1 |  |
| `integrations.backfill_github_external_actor.gh_api_fetch_interval_s` | Float | `0.1` | 1 |  |
| `integrations.slo.integration-id-tag-enabled` | Bool | `False` | 1 |  |
| `issue-detection.llm-detection.enabled` | Bool | `False` | 1 |  |
| `issue-detection.llm-detection.traces-per-invocation` | Dict | `{"team": 1` | 2 |  |
| `issue-detection.web-vitals-detection.enabled` | Bool | `False` | 2 |  |
| `issue-detection.web-vitals-detection.projects-allowlist` | Sequence | `[]` | 1 |  |
| `issues.client_error_sampling.project_allowlist` | Sequence | `[]` | 9 |  |
| `issues.group_events.batch_nodestore_enabled` | Bool | `True` | 1 |  |
| `issues.occurrence-consumer.rate-limit.enabled` | Bool | `False` | 2 |  |
| `issues.occurrence-consumer.rate-limit.quota` | Dict | `{"window_seconds": 3600` | 2 |  |
| `issues.record-seer-actions-as-activities` | Bool | `True` | 2 |  |
| `issues.sdk_crash_detection.cocoa.project_id` | inferred | `4505469596663808` | 6 |  |
| `issues.sdk_crash_detection.cocoa.sample_rate` | inferred | `1.0` | 6 |  |
| `issues.sdk_crash_detection.dart.organization_allowlist` | Sequence | `[]` | 2 |  |
| `issues.sdk_crash_detection.dart.project_id` | Int | `0` | 2 |  |
| `issues.sdk_crash_detection.dart.sample_rate` | inferred | `0.0` | 2 |  |
| `issues.sdk_crash_detection.dotnet.organization_allowlist` | Sequence | `[]` | 2 |  |
| `issues.sdk_crash_detection.dotnet.project_id` | Int | `0` | 2 |  |
| `issues.sdk_crash_detection.dotnet.sample_rate` | inferred | `0.0` | 2 |  |
| `issues.sdk_crash_detection.java.organization_allowlist` | Sequence | `[]` | 3 |  |
| `issues.sdk_crash_detection.java.project_id` | Int | `0` | 3 |  |
| `issues.sdk_crash_detection.java.sample_rate` | inferred | `0.0` | 3 |  |
| `issues.sdk_crash_detection.native.organization_allowlist` | Sequence | `[]` | 2 |  |
| `issues.sdk_crash_detection.native.project_id` | Int | `0` | 2 |  |
| `issues.sdk_crash_detection.native.sample_rate` | inferred | `0.0` | 2 |  |
| `issues.sdk_crash_detection.react-native.organization_allowlist` | Sequence | `[]` | 4 |  |
| `issues.sdk_crash_detection.react-native.project_id` | inferred | `4506155486085120` | 5 |  |
| `issues.sdk_crash_detection.react-native.sample_rate` | inferred | `0.0` | 4 |  |
| `issues.severity.seer-circuit-breaker-passthrough-limit` | Dict | `{"limit": 1` | 1 |  |
| `issues.severity.seer-global-rate-limit` | Any | `{"limit": 20` | 1 |  |
| `issues.severity.seer-project-rate-limit` | Any | `{"limit": 5` | 1 |  |
| `issues.severity.seer-timeout` | Float | `0.2` | 2 |  |
| `issues.severity.skip-seer-requests` | Sequence | `[]` | 4 |  |
| `kafka.send-project-events-to-random-partitions` | inferred | `[]` | 2 |  |
| `mail.enable-replies` | inferred | `False` | 11 | DISK |
| `mail.mailgun-api-key` | inferred | `""` | 10 | DISK |
| `mail.reply-hostname` | inferred | `""` | 10 | DISK |
| `mail.subject-prefix` | inferred | `"[Sentry]"` | 19 | DISK |
| `mail.timeout` | Int | `10` | 2 | DISK |
| `metric_alerts.extended_max_subscriptions` | inferred | `1250` | 5 |  |
| `metric_alerts.extended_max_subscriptions_orgs` | inferred | `[]` | 5 |  |
| `msteams.client-id` | inferred | `` | 7 | DISK |
| `nodestore.cache-ttl` | Int | `300` | 2 |  |
| `nodestore.set-subkeys.enable-set-cache-item` | inferred | `True` | 2 |  |
| `notifications.platform-rollout.early-adopter` | Dict | `{}` | 1 |  |
| `notifications.platform-rollout.general-access` | Dict | `{}` | 1 |  |
| `notifications.platform-rollout.internal-testing` | Dict | `{}` | 6 |  |
| `notifications.platform-rollout.is-sentry` | Dict | `{}` | 2 |  |
| `notifications.platform.killswitch.sources` | Sequence | `[]` | 1 |  |
| `objectstore.enable_for.attachments` | inferred | `0.0` | 4 |  |
| `on_demand.extended_alert_spec_orgs` | inferred | `[]` | 1 |  |
| `on_demand.extended_max_alert_specs` | inferred | `750` | 1 |  |
| `on_demand.extended_max_widget_specs` | inferred | `750` | 2 |  |
| `on_demand.extended_widget_spec_orgs` | inferred | `[]` | 2 |  |
| `on_demand.max_alert_specs` | inferred | `50` | 2 |  |
| `on_demand.max_widget_cardinality.count` | inferred | `10000` | 4 |  |
| `on_demand.max_widget_cardinality.killswitch` | inferred | `False` | 1 |  |
| `on_demand.max_widget_cardinality.on_query_count` | inferred | `50` | 2 |  |
| `on_demand.max_widget_specs` | inferred | `100` | 2 |  |
| `on_demand.update_on_demand_modified` | inferred | `False` | 1 |  |
| `on_demand_metrics.cache_should_use_on_demand` | inferred | `0.0` | 3 |  |
| `on_demand_metrics.check_widgets.enable` | inferred | `False` | 3 | DISK |
| `on_demand_metrics.check_widgets.query.batch_size` | Int | `50` | 2 | DISK |
| `on_demand_metrics.check_widgets.query.total_batches` | inferred | `100` | 2 | DISK |
| `on_demand_metrics.check_widgets.rollout` | Float | `0.0` | 2 | DISK |
| `on_demand_metrics.widgets.use_stateful_extraction` | inferred | `False` | 2 | DISK |
| `options_automator_slack_webhook_enabled` | inferred | `True` | 1 |  |
| `organization-abuse-quota.metric-bucket-limit` | Int | `0` | 1 | DISK |
| `organization.default-owner-id-cache-ttl` | Int | `300` | 1 |  |
| `ourlogs.sentry-emit-rollout` | inferred | `0.0` | 1 |  |
| `outbox_replication.accounts_thirdpartyaccount.replication_version` | Int | `0` | 0 |  |
| `outbox_replication.auth_authenticator.replication_version` | Int | `0` | 0 |  |
| `outbox_replication.auth_user.replication_version` | Int | `0` | 0 |  |
| `outbox_replication.sentry_apikey.replication_version` | Int | `0` | 0 |  |
| `outbox_replication.sentry_apitoken.replication_version` | Int | `0` | 0 |  |
| `outbox_replication.sentry_authidentity.replication_version` | Int | `0` | 0 |  |
| `outbox_replication.sentry_authprovider.replication_version` | Int | `0` | 1 |  |
| `outbox_replication.sentry_externalactor.replication_version` | Int | `0` | 0 |  |
| `outbox_replication.sentry_organization.replication_version` | Int | `0` | 1 |  |
| `outbox_replication.sentry_organizationavatar.replication_version` | Int | `0` | 0 |  |
| `outbox_replication.sentry_organizationintegration.replication_version` | Int | `0` | 0 |  |
| `outbox_replication.sentry_organizationmember.replication_version` | Int | `0` | 0 |  |
| `outbox_replication.sentry_organizationmember_teams.replication_version` | Int | `0` | 0 |  |
| `outbox_replication.sentry_organizationslugreservation.replication_version` | Int | `0` | 0 |  |
| `outbox_replication.sentry_orgauthtoken.replication_version` | Int | `0` | 0 |  |
| `outbox_replication.sentry_projectkey.replication_version` | Int | `0` | 0 |  |
| `outbox_replication.sentry_sentryappinstallation.replication_version` | Int | `0` | 0 |  |
| `outbox_replication.sentry_sentryappinstallationtoken.replication_version` | Int | `0` | 0 |  |
| `outbox_replication.sentry_team.replication_version` | Int | `0` | 0 |  |
| `outbox_replication.sentry_useremail.replication_version` | Int | `0` | 0 |  |
| `outbox_replication.sentry_userpermission.replication_version` | Int | `0` | 0 |  |
| `outbox_replication.sentry_userrole.replication_version` | Int | `0` | 0 |  |
| `outbox_replication.sentry_userrole_users.replication_version` | Int | `0` | 0 |  |
| `outcomes_consumer.usage_buffer.allowlist.rollout` | Sequence | `[]` | 1 |  |
| `outcomes_consumer.usage_buffer.percent.rollout` | inferred | `0.0` | 1 |  |
| `outcomes_consumer.usage_buffer.recover_orphaned_data.enable` | Bool | `False` | 2 |  |
| `outcomes_consumer.usage_buffer.recover_orphaned_data.limit` | Int | `1000` | 2 |  |
| `pagerduty.app-id` | inferred | `""` | 5 |  |
| `performance.event-tracker.sample-rate.transactions` | inferred | `0.0` | 1 |  |
| `performance.extrapolation.confidence.z-score` | Float | `1.96` | 1 |  |
| `performance.issues.all.problem-detection` | inferred | `1.0` | 6 |  |
| `performance.issues.compressed_assets.problem-creation` | inferred | `1.0` | 1 |  |
| `performance.issues.consecutive_db.min_time_saved_threshold` | inferred | `100` | 2 |  |
| `performance.issues.consecutive_db.problem-creation` | inferred | `1.0` | 1 |  |
| `performance.issues.consecutive_http.consecutive_count_threshold` | inferred | `3` | 1 |  |
| `performance.issues.consecutive_http.max_duration_between_spans` | inferred | `500` | 1 |  |
| `performance.issues.consecutive_http.min_time_saved_threshold` | inferred | `2000` | 2 |  |
| `performance.issues.consecutive_http.problem-creation` | inferred | `1.0` | 2 |  |
| `performance.issues.consecutive_http.span_duration_threshold` | inferred | `500` | 1 |  |
| `performance.issues.db_main_thread.problem-creation` | inferred | `1.0` | 1 |  |
| `performance.issues.db_on_main_thread.total_spans_duration_threshold` | inferred | `16` | 2 |  |
| `performance.issues.file_io_main_thread.problem-creation` | inferred | `1.0` | 2 |  |
| `performance.issues.file_io_on_main_thread.total_spans_duration_threshold` | inferred | `16` | 2 |  |
| `performance.issues.http_overhead.http_request_delay_threshold` | inferred | `250` | 1 |  |
| `performance.issues.http_overhead.problem-creation` | inferred | `1.0` | 1 |  |
| `performance.issues.large_http_payload.filtered_paths` | inferred | `""` | 1 |  |
| `performance.issues.large_http_payload.problem-creation` | inferred | `1.0` | 1 |  |
| `performance.issues.large_http_payload.size_threshold` | inferred | `300000` | 2 |  |
| `performance.issues.m_n_plus_one_db.problem-creation` | inferred | `1.0` | 1 |  |
| `performance.issues.n_plus_one_api_calls.problem-creation` | inferred | `1.0` | 2 |  |
| `performance.issues.n_plus_one_api_calls.total_duration` | inferred | `300` | 2 |  |
| `performance.issues.n_plus_one_db.count_threshold` | inferred | `5` | 2 |  |
| `performance.issues.n_plus_one_db.duration_threshold` | inferred | `50.0` | 3 |  |
| `performance.issues.n_plus_one_db.problem-creation` | inferred | `1.0` | 5 |  |
| `performance.issues.query_injection.problem-creation` | inferred | `0.0` | 1 |  |
| `performance.issues.render_blocking_assets.fcp_maximum_threshold` | inferred | `10000.0` | 1 |  |
| `performance.issues.render_blocking_assets.fcp_minimum_threshold` | inferred | `2000.0` | 1 |  |
| `performance.issues.render_blocking_assets.fcp_ratio_threshold` | inferred | `0.33` | 2 |  |
| `performance.issues.render_blocking_assets.problem-creation` | inferred | `1.0` | 1 |  |
| `performance.issues.render_blocking_assets.size_threshold` | inferred | `500000` | 1 |  |
| `performance.issues.slow_db_query.duration_threshold` | inferred | `1000.0` | 2 |  |
| `performance.issues.slow_db_query.problem-creation` | inferred | `1.0` | 4 |  |
| `performance.issues.sql_injection.problem-creation` | inferred | `0.0` | 1 |  |
| `performance.issues.sql_injection.query_value_length_threshold` | inferred | `3` | 1 |  |
| `performance.issues.uncompressed_asset.duration_threshold` | inferred | `300` | 2 |  |
| `performance.issues.uncompressed_asset.size_threshold` | inferred | `500 * 1024` | 2 |  |
| `performance.issues.web_vitals.count_threshold` | inferred | `10` | 2 |  |
| `performance.spans-tags-key.max` | Int | `1000` | 1 |  |
| `performance.spans-tags-values.max` | Int | `1000` | 1 |  |
| `performance.trace.span_with_errors_ok_status.sample_rate` | Float | `0.0` | 1 |  |
| `performance.traces.check_span_extraction_date` | Bool | `False` | 1 |  |
| `performance.traces.pagination.max-iterations` | Int | `1` | 2 |  |
| `performance.traces.pagination.max-timeout` | Float | `0.0` | 2 |  |
| `performance.traces.pagination.query-limit` | Int | `10_000` | 2 |  |
| `performance.traces.query_timestamp_projects` | Bool | `False` | 1 |  |
| `performance.traces.span_query_timebuffer_hours` | Float | `1.0` | 1 |  |
| `performance.traces.trace-explorer-skip-recent-seconds` | Int | `0` | 1 |  |
| `performance.traces.transaction_query_timebuffer_days` | Float | `1.5` | 2 |  |
| `post-process-forwarder:kafka-headers` | inferred | `True` | 1 |  |
| `post_process.get-autoassign-owners` | Sequence | `[]` | 2 |  |
| `processing.severity-backlog-test.error` | inferred | `False` | 1 |  |
| `processing.severity-backlog-test.timeout` | inferred | `False` | 1 |  |
| `profiling.continuous-profiling.chunks-query.size` | Int | `250` | 1 |  |
| `profiling.continuous-profiling.chunks-set.size` | Int | `50` | 2 |  |
| `profiling.flamegraph.profile-set.size` | Int | `100` | 2 |  |
| `profiling.flamegraph.query.initial_chunk_delta.hours` | Int | `12` | 1 |  |
| `profiling.flamegraph.query.max_delta.hours` | Int | `48` | 1 |  |
| `profiling.flamegraph.query.multiplier` | Int | `2` | 1 |  |
| `profiling.killswitch.ingest-profiles` | Sequence | `[]` | 3 | DISK |
| `profiling.profile_metrics.unsampled_profiles.enabled` | Bool | `False` | 4 |  |
| `profiling.profile_metrics.unsampled_profiles.platforms` | Sequence | `[]` | 2 |  |
| `profiling.profile_metrics.unsampled_profiles.sample_rate` | inferred | `0.0` | 2 |  |
| `project-abuse-quota.attachment-item-limit` | Int | `0` | 3 | DISK |
| `project-abuse-quota.attachment-limit` | Int | `0` | 3 | DISK |
| `project-abuse-quota.error-limit` | Int | `0` | 3 | DISK |
| `project-abuse-quota.session-limit` | Int | `0` | 3 | DISK |
| `project-abuse-quota.span-limit` | Int | `0` | 1 | DISK |
| `project-abuse-quota.transaction-limit` | Int | `0` | 1 | DISK |
| `project-abuse-quota.window` | Int | `10` | 2 | DISK |
| `provision_organization.override.mapping` | Dict | `{}` | 2 |  |
| `provision_organization.override.rate` | Float | `0.0` | 2 |  |
| `recovery.disallow-new-enrollment` | Bool | `False` | 0 | DISK |
| `relay.drop-transaction-attachments` | Bool | `False` | 1 |  |
| `relay.drop-transaction-metrics` | inferred | `[]` | 2 |  |
| `relay.eap-outcomes.rollout-rate` | Float | `0.0` | 2 |  |
| `relay.eap-span-outcomes.rollout-rate` | Float | `0.0` | 2 |  |
| `relay.endpoint-fetch-config.enabled` | Bool | `True` | 1 |  |
| `relay.invalidation-direct-outside-atomic` | inferred | `False` | 2 |  |
| `relay.kafka.span-v2.sample-rate` | Float | `0.0` | 1 |  |
| `relay.metric-bucket-distribution-encodings` | inferred | `{}` | 2 |  |
| `relay.metric-bucket-set-encodings` | inferred | `{}` | 2 |  |
| `relay.objectstore-attachments.sample-rate` | Float | `0.0` | 2 |  |
| `relay.projectconfigs.migration.rollout` | inferred | `0.0` | 1 |  |
| `relay.quotas.migration.rollout` | inferred | `0.0` | 2 |  |
| `relay.sessions-eap.rollout-rate` | Float | `0.0` | 2 |  |
| `relay.span-normalization.allowed_hosts` | inferred | `[]` | 2 |  |
| `relay.span-usage-metric` | inferred | `False` | 2 |  |
| `release-health.disable-release-last-seen-update` | Bool | `False` | 2 |  |
| `release-health.monitor-release-adoption-jitter-seconds` | Int | `45 * 60` | 2 |  |
| `release-health.use-org-and-project-filter` | Bool | `False` | 2 |  |
| `releasefile.cache-path` | String | `"/tmp/sentry-releasefile-cache` | 2 | DISK |
| `releases.no_snuba_for_release_creation` | Bool | `False` | 2 |  |
| `relocation.autopause` | inferred | `""` | 3 |  |
| `relocation.autopause.saas-to-saas` | inferred | `""` | 2 |  |
| `relocation.autopause.self-hosted` | inferred | `""` | 2 |  |
| `relocation.daily-limit.large` | inferred | `0` | 0 |  |
| `relocation.daily-limit.medium` | inferred | `0` | 1 |  |
| `relocation.daily-limit.small` | inferred | `0` | 3 |  |
| `relocation.enabled` | inferred | `False` | 7 |  |
| `relocation.selectable-regions` | inferred | `[]` | 1 |  |
| `replay.consumer.enable_new_query_caching_system` | Bool | `False` | 2 |  |
| `replay.consumer.msgspec_recording_parser` | Bool | `False` | 1 |  |
| `replay.endpoints.project_replay_summary.trace_sample_rate_get` | inferred | `0.0` | 1 |  |
| `replay.endpoints.project_replay_summary.trace_sample_rate_post` | inferred | `0.0` | 1 |  |
| `replay.recording.ingest-trace-items.rollout` | Float | `0.0` | 1 | DISK |
| `replay.replay-video.disabled` | Bool | `False` | 2 | DISK |
| `replay.replay-video.slug-denylist` | Sequence | `[]` | 2 | DISK |
| `replay.storage.backend` | inferred | `None` | 3 | DISK |
| `replay.storage.options` | Dict | `None` | 3 | DISK |
| `replay.viewed-by.project-denylist` | Sequence | `[]` | 4 | DISK |
| `repository.auto-link-by-name-dry-run` | Bool | `True` | 3 |  |
| `reprocessing2.drop-delete-old-primary-hash` | inferred | `[]` | 2 |  |
| `sdk-deprecation.profile-chunk.cocoa` | inferred | `"8.49.2"` | 2 |  |
| `sdk-deprecation.profile-chunk.cocoa.hard` | inferred | `"8.49.0"` | 1 |  |
| `sdk-deprecation.profile-chunk.cocoa.reject` | inferred | `"8.49.2"` | 1 |  |
| `sdk-deprecation.profile-chunk.python` | inferred | `"2.24.1"` | 2 |  |
| `sdk-deprecation.profile-chunk.python.hard` | inferred | `"2.24.1"` | 1 |  |
| `sdk-deprecation.profile.cocoa.reject` | inferred | `"8.49.2"` | 1 |  |
| `sdk_http2_experiment.enabled` | Bool | `False` | 1 |  |
| `secret-scanning.github.enable-signature-verification` | Bool | `True` | 2 |  |
| `seer.api.use-shared-secret` | inferred | `0.0` | 1 |  |
| `seer.code-review.excluded-pr-author-logins` | Sequence | `[]` | 2 |  |
| `seer.explorer.context-engine-rollout` | Float | `0.0` | 1 |  |
| `seer.explorer_index.killswitch.enable` | Bool | `False` | 4 |  |
| `seer.global-killswitch.enabled` | Bool | `False` | 2 |  |
| `seer.max_num_autofix_autotriggered_per_hour` | inferred | `20` | 2 |  |
| `seer.max_num_scanner_autotriggered_per_ten_seconds` | inferred | `15` | 2 |  |
| `seer.night_shift.enable` | Bool | `False` | 2 |  |
| `seer.night_shift.issues_per_org` | inferred | `10` | 5 |  |
| `seer.organizations.force-config-reminder` | Sequence | `[]` | 2 |  |
| `seer.similarity-embeddings-delete-by-hash-killswitch.enabled` | Bool | `False` | 1 |  |
| `seer.similarity-embeddings-killswitch.enabled` | Bool | `False` | 2 |  |
| `seer.similarity-killswitch.enabled` | Bool | `False` | 2 |  |
| `seer.similarity.circuit-breaker-config` | Dict | `{         "error_limit": 33250` | 2 |  |
| `seer.similarity.global-rate-limit` | Dict | `{"limit": 20` | 1 |  |
| `seer.similarity.grouping-ingest-retries` | Int | `0` | 2 |  |
| `seer.similarity.grouping-ingest-timeout` | Int | `1` | 2 |  |
| `seer.similarity.grouping_killswitch_projects` | Sequence | `[]` | 3 |  |
| `seer.similarity.ingest.num_matches_to_request` | Int | `1` | 2 |  |
| `seer.similarity.ingest.store_hybrid_fingerprint_non_matches` | Bool | `True` | 1 |  |
| `seer.similarity.max_token_count` | Int | `7000` | 3 |  |
| `seer.similarity.metrics_sample_rate` | Float | `1.0` | 9 |  |
| `seer.similarity.per-project-rate-limit` | Dict | `{"limit": 5` | 1 |  |
| `seer.supergroups_backfill_lightweight.batch_size` | Int | `40` | 2 |  |
| `seer.supergroups_backfill_lightweight.inter_batch_delay_s` | Int | `5` | 1 |  |
| `seer.supergroups_backfill_lightweight.killswitch` | Bool | `False` | 2 |  |
| `seer.supergroups_backfill_lightweight.max_failures_per_batch` | Int | `20` | 1 |  |
| `sentry-apps.disable-paranoia` | Bool | `False` | 4 |  |
| `sentry-apps.disabled-enforcement` | Bool | `False` | 4 |  |
| `sentry-apps.expanded-webhook-categories` | Sequence | `[         1` | 1 |  |
| `sentry-apps.hard-delete` | Bool | `False` | 5 |  |
| `sentry-apps.legacy-webhook-payload-validation.rate` | Float | `0.0` | 1 |  |
| `sentry-apps.webhook-logging.enabled` | Dict | `{         "sentry_app_slug": [` | 2 |  |
| `sentry-apps.webhook.circuit-breaker.config` | Dict | `{         "error_limit_window"` | 2 |  |
| `sentry-apps.webhook.hard-timeout.sec` | inferred | `5.0` | 2 |  |
| `sentry-apps.webhook.restricted-webhook-sending` | Sequence | `[]` | 3 |  |
| `sentry-apps.webhook.timeout.sec` | inferred | `1.0` | 3 |  |
| `sentry-metrics.10s-granularity` | inferred | `False` | 2 |  |
| `sentry-metrics.cardinality-limiter.limits.custom.per-org` | inferred | `[         {"window_seconds": 3` | 0 |  |
| `sentry-metrics.cardinality-limiter.limits.generic-metrics.per-org` | inferred | `[         {"window_seconds": 3` | 0 |  |
| `sentry-metrics.cardinality-limiter.limits.profiles.per-org` | inferred | `[         {"window_seconds": 3` | 0 |  |
| `sentry-metrics.cardinality-limiter.limits.sessions.per-org` | inferred | `[         {"window_seconds": 3` | 0 |  |
| `sentry-metrics.cardinality-limiter.limits.spans.per-org` | inferred | `[         {"window_seconds": 3` | 0 |  |
| `sentry-metrics.cardinality-limiter.limits.transactions.per-org` | inferred | `[         {"window_seconds": 3` | 0 |  |
| `sentry-metrics.drop-percentiles.per-use-case` | inferred | `[]` | 3 |  |
| `sentry-metrics.indexer.disable-memcache-replenish-rollout` | inferred | `0.0` | 1 |  |
| `sentry-metrics.indexer.disabled-namespaces` | inferred | `[]` | 2 |  |
| `sentry-metrics.indexer.generic-metrics.schema-validation-rules` | inferred | `{}` | 1 |  |
| `sentry-metrics.indexer.read-new-cache-namespace` | inferred | `False` | 3 |  |
| `sentry-metrics.indexer.reconstruct.enable-orjson` | inferred | `0.0` | 1 |  |
| `sentry-metrics.indexer.release-health.schema-validation-rules` | inferred | `{}` | 1 |  |
| `sentry-metrics.indexer.write-new-cache-namespace` | inferred | `False` | 3 |  |
| `sentry-metrics.releasehealth.abnormal-mechanism-extraction-rate` | inferred | `0.0` | 3 |  |
| `sentry-metrics.synchronized-rebalance-delay` | inferred | `15` | 1 |  |
| `sentry-metrics.writes-limiter.limits.custom.global` | inferred | `[]` | 0 |  |
| `sentry-metrics.writes-limiter.limits.custom.per-org` | inferred | `[]` | 0 |  |
| `sentry-metrics.writes-limiter.limits.generic-metrics.global` | inferred | `[]` | 1 |  |
| `sentry-metrics.writes-limiter.limits.generic-metrics.per-org` | inferred | `[]` | 1 |  |
| `sentry-metrics.writes-limiter.limits.performance.global` | inferred | `[]` | 1 |  |
| `sentry-metrics.writes-limiter.limits.performance.per-org` | inferred | `[]` | 1 |  |
| `sentry-metrics.writes-limiter.limits.releasehealth.global` | inferred | `[]` | 1 |  |
| `sentry-metrics.writes-limiter.limits.releasehealth.per-org` | inferred | `[]` | 1 |  |
| `sentry-metrics.writes-limiter.limits.sessions.global` | inferred | `[]` | 0 |  |
| `sentry-metrics.writes-limiter.limits.sessions.per-org` | inferred | `[]` | 0 |  |
| `sentry-metrics.writes-limiter.limits.spans.global` | inferred | `[]` | 1 |  |
| `sentry-metrics.writes-limiter.limits.spans.per-org` | inferred | `[]` | 1 |  |
| `sentry-metrics.writes-limiter.limits.transactions.global` | inferred | `[]` | 1 |  |
| `sentry-metrics.writes-limiter.limits.transactions.per-org` | inferred | `[]` | 1 |  |
| `sentry.demo_mode.sync_debug_artifacts.enable` | Bool | `False` | 1 | DISK |
| `sentry.demo_mode.sync_debug_artifacts.source_org_id` | Int | `` | 1 | DISK |
| `sentry.save-event-attachments.project-per-5-minute-limit` | Int | `2000` | 1 |  |
| `sentry.save-event-attachments.project-per-sec-limit` | Int | `100` | 1 |  |
| `sentry.scm.stream.rollout` | Float | `0.0` | 1 | DISK |
| `sentry.search.events.project.check_event` | Float | `0.0` | 1 |  |
| `sentry.send_onboarding_task_metrics` | Bool | `False` | 1 |  |
| `sentry.similarity.indexing.enabled` | Bool | `True` | 2 |  |
| `sentry:skip-record-onboarding-tasks-if-complete` | Bool | `False` | 1 |  |
| `similarity.new_project_seer_grouping.enabled` | inferred | `False` | 2 |  |
| `slack-staging.client-id` | inferred | `` | 3 | DISK |
| `slack.client-id` | inferred | `` | 6 | DISK |
| `slack.debug-channel` | inferred | `` | 1 |  |
| `slack.debug-workspace` | inferred | `` | 1 |  |
| `slack.log-unfurl-payload` | inferred | `False` | 1 |  |
| `sms.disallow-new-enrollment` | Bool | `False` | 3 |  |
| `sms.twilio-account` | inferred | `""` | 5 | DISK |
| `sms.twilio-number` | inferred | `""` | 2 | DISK |
| `snuba.groupsnooze.user-counts-debounce-seconds` | Int | `0` | 2 |  |
| `snuba.search.chunk-growth-rate` | inferred | `1.5` | 1 |  |
| `snuba.search.hits-sample-size` | inferred | `100` | 2 |  |
| `snuba.search.max-chunk-size` | inferred | `2000` | 1 |  |
| `snuba.search.max-pre-snuba-candidates` | inferred | `5000` | 2 |  |
| `snuba.search.max-total-chunk-time-seconds` | inferred | `30.0` | 1 |  |
| `snuba.search.min-pre-snuba-candidates` | inferred | `500` | 1 |  |
| `snuba.search.pre-snuba-candidates-optimizer` | Bool | `False` | 1 |  |
| `snuba.search.recommended.event-volume-weight` | inferred | `0.20` | 1 |  |
| `snuba.search.recommended.group-type-boost` | Dict | `{7001: 0.15}` | 2 |  |
| `snuba.search.recommended.recency-weight` | inferred | `0.20` | 1 |  |
| `snuba.search.recommended.severity-weight` | inferred | `0.20` | 1 |  |
| `snuba.search.recommended.spike-weight` | inferred | `0.20` | 1 |  |
| `snuba.search.recommended.user-impact-weight` | inferred | `0.05` | 1 |  |
| `snuba.tagstore.cache-tagkeys-rate` | inferred | `0.0` | 1 | DISK |
| `span-metrics-extraction-addons-enabled` | inferred | `False` | 2 |  |
| `span-metrics-extraction-addons-orgs-denylist` | Sequence | `[]` | 2 |  |
| `span-metrics-extraction-addons-projects-denylist` | Sequence | `[]` | 2 |  |
| `span-metrics-extraction-enabled` | inferred | `False` | 2 |  |
| `span-metrics-extraction-orgs-denylist` | Sequence | `[]` | 2 |  |
| `span-metrics-extraction-projects-denylist` | Sequence | `[]` | 2 |  |
| `spans.buffer.compression.level` | Int | `0` | 2 | DISK |
| `spans.buffer.debug-traces` | Sequence | `[]` | 3 |  |
| `spans.buffer.evalsha-cumulative-logger-enabled` | inferred | `False` | 3 | DISK |
| `spans.buffer.evalsha-latency-threshold` | Int | `100` | 1 |  |
| `spans.buffer.flusher-cumulative-logger-enabled` | inferred | `False` | 3 | DISK |
| `spans.buffer.flusher.backpressure-seconds` | inferred | `10` | 2 | DISK |
| `spans.buffer.flusher.flush-lock-ttl` | Int | `0` | 4 | DISK |
| `spans.buffer.flusher.log-flushed-segments` | inferred | `False` | 3 | DISK |
| `spans.buffer.flusher.max-unhealthy-seconds` | inferred | `60` | 3 | DISK |
| `spans.buffer.flusher.use-stuck-detector` | inferred | `False` | 3 | DISK |
| `spans.buffer.max-flush-segments` | Int | `500` | 3 | DISK |
| `spans.buffer.max-memory-percentage` | Float | `1.0` | 2 | DISK |
| `spans.buffer.max-segment-bytes` | Int | `10 * 1024 * 1024` | 4 | DISK |
| `spans.buffer.max-spans-per-evalsha` | Int | `0` | 2 | DISK |
| `spans.buffer.pipeline-batch-size` | Int | `0` | 2 | DISK |
| `spans.buffer.redis-ttl` | Int | `3600` | 2 | DISK |
| `spans.buffer.root-timeout` | Int | `10` | 3 | DISK |
| `spans.buffer.segment-page-size` | Int | `100` | 3 | DISK |
| `spans.buffer.timeout` | Int | `60` | 3 | DISK |
| `spans.drop-in-buffer` | Sequence | `[]` | 3 | DISK |
| `spans.process-segments.consumer.enable` | inferred | `True` | 2 | DISK |
| `spans.process-segments.dedupe-filter-enable` | inferred | `False` | 2 | DISK |
| `spans.process-segments.dedupe-ttl` | Int | `0` | 2 | DISK |
| `spans.process-segments.detect-performance-problems.enable` | inferred | `False` | 2 | DISK |
| `spans.process-segments.drop-segments` | Sequence | `[]` | 3 |  |
| `spans.process-segments.schema-validation` | inferred | `0.0` | 3 |  |
| `spans.process-segments.skip-enrichment-projects` | Sequence | `[]` | 2 |  |
| `spans.process-spans.profiling.rate` | Float | `0.0` | 1 | DISK |
| `staff.ga-rollout` | Bool | `False` | 44 |  |
| `staff.user-email-allowlist` | Sequence | `[]` | 1 |  |
| `standalone-span-discard-transaction` | inferred | `False` | 2 |  |
| `standalone-span-discard-transaction-project-allowlist` | Sequence | `[]` | 2 |  |
| `statistical_detectors.enable` | inferred | `False` | 2 | DISK |
| `statistical_detectors.query.batch_size` | Int | `100` | 1 | DISK |
| `statistical_detectors.query.functions.timeseries_days` | Int | `14` | 1 | DISK |
| `statistical_detectors.query.transactions.timeseries_days` | Int | `14` | 1 | DISK |
| `statistical_detectors.ratelimit.ema` | Int | `-1` | 2 | DISK |
| `statistical_detectors.throughput.threshold.functions` | Int | `25` | 1 |  |
| `statistical_detectors.throughput.threshold.transactions` | Int | `50` | 1 |  |
| `store.allow-s4s-ddm-sample-rate` | inferred | `0.0` | 1 |  |
| `store.load-shed-group-creation-projects` | Any | `[]` | 3 |  |
| `store.load-shed-parsed-pipeline-projects` | Any | `[]` | 2 |  |
| `store.load-shed-pipeline-projects` | Any | `[]` | 3 |  |
| `store.load-shed-process-event-projects` | Any | `[]` | 3 |  |
| `store.load-shed-process-event-projects-gradual` | Dict | `{}` | 2 |  |
| `store.load-shed-save-event-projects` | Any | `[]` | 2 |  |
| `store.load-shed-symbolicate-event-projects` | Any | `[]` | 3 |  |
| `store.reprocessing-force-disable` | inferred | `False` | 1 |  |
| `store.s4s-transaction-sample-rate` | inferred | `1.0` | 2 |  |
| `store.use-relay-dsn-sample-rate` | inferred | `1` | 1 |  |
| `subscriptions-query.sample-rate` | inferred | `0.01` | 1 |  |
| `superuser.read-write.ga-rollout` | Bool | `False` | 20 |  |
| `symbolicate.symx-logging-rate` | inferred | `0.0` | 1 |  |
| `symbolicate.symx-os-description-list` | inferred | `[]` | 1 |  |
| `symbolicator.enabled` | inferred | `False` | 7 | DISK |
| `symbolicator.ignored_sources` | Sequence | `[]` | 2 |  |
| `symbolicator.options` | inferred | `{"url": "http://127.0.0.1:3021` | 4 | DISK |
| `symbolicator.sourcemaps-bundle-index-refresh-sample-rate` | inferred | `0.0` | 1 |  |
| `symbolserver.enabled` | inferred | `False` | 6 | DISK |
| `symbolserver.options` | inferred | `{"url": "http://127.0.0.1:3000` | 1 | DISK |
| `system.debug-files-renewal-age-threshold-days` | inferred | `30` | 3 |  |
| `system.event-retention-days` | inferred | `0` | 23 | DISK |
| `system.internal-url-prefix` | inferred | `` | 9 | DISK |
| `system.security-email` | inferred | `` | 8 | DISK |
| `system.support-email` | inferred | `` | 8 | DISK |
| `system.upload-url-prefix` | inferred | `` | 2 | DISK |
| `tally-usage-delay-seconds` | Int | `0` | 3 |  |
| `taskworker.producer.max_futures` | Int | `1000` | 1 |  |
| `taskworker.route.overrides` | inferred | `{}` | 2 |  |
| `tempest.crashes-timeout` | inferred | `55` | 2 |  |
| `tempest.latest-id-timeout` | inferred | `55` | 2 |  |
| `tempest.lock-buffer-seconds` | inferred | `30` | 2 |  |
| `tempest.poll-limit` | inferred | `25` | 1 |  |
| `tempest.task-deadline-seconds` | inferred | `60` | 2 |  |
| `tempest.tempest-ips-api-response` | Sequence | `[]` | 2 |  |
| `totp.disallow-new-enrollment` | Bool | `False` | 1 | DISK |
| `transaction-events.force-disable-internal-project` | inferred | `False` | 1 |  |
| `txnames.bump-lifetime-sample-rate` | inferred | `0.1` | 2 |  |
| `u2f.app-id` | inferred | `""` | 1 | DISK |
| `u2f.disallow-new-enrollment` | Bool | `False` | 1 | DISK |
| `u2f.facets` | Sequence | `[]` | 4 | DISK |
| `uptime.automatic-hostname-detection` | Bool | `True` | 3 |  |
| `uptime.automatic-subscription-creation` | Bool | `True` | 2 |  |
| `uptime.checker-regions-mode-override` | Dict | `{}` | 6 |  |
| `uptime.create-issues` | Bool | `True` | 3 |  |
| `uptime.restrict-issue-creation-by-hosting-provider-id` | Sequence | `[]` | 2 |  |
| `uptime.update-checker-script-interval-seconds` | inferred | `300` | 2 |  |
| `uptime.uptime-ips-api-response` | Sequence | `[]` | 2 |  |
| `uptime.use-detectors-by-data-source-cache` | Bool | `True` | 1 |  |
| `user-settings.signed-url-confirmation-emails` | inferred | `False` | 3 | DISK |
| `user-settings.signed-url-confirmation-emails-salt` | String | `"signed-url-confirmation-email` | 3 | DISK |
| `vercel.client-id` | inferred | `` | 6 | DISK |
| `vercel.integration-slug` | inferred | `"sentry"` | 1 |  |
| `vercel.invoice-notpaid.disable-downgrade` | inferred | `False` | 2 |  |
| `visibility.tag-key-max-date-range.days` | inferred | `14` | 2 |  |
| `visibility.tag-key-sample-size` | inferred | `1_000_000` | 1 |  |
| `vsts-limited.client-id` | inferred | `` | 4 | DISK |
| `vsts.client-id` | inferred | `` | 4 | DISK |
| `vsts.consent-prompt` | inferred | `False` | 3 |  |
| `vsts.social-auth-migration` | Bool | `False` | 1 |  |
| `vsts_new.client-id` | inferred | `` | 2 | DISK |
| `workflow_engine.associate_error_detectors` | Bool | `False` | 2 |  |
| `workflow_engine.ensure_detector_association` | Bool | `True` | 2 |  |
| `workflow_engine.evaluation_log_sample_rate` | Float | `0.1` | 3 |  |
| `workflow_engine.evaluation_logs_direct_to_sentry` | Bool | `False` | 2 |  |
| `workflow_engine.filter_cross_org_workflows` | Bool | `True` | 2 |  |
| `workflow_engine.group.type_id.disable_issue_stream_detector` | Sequence | `[8001]` | 2 |  |
| `workflow_engine.group.type_id.open_periods_type_denylist` | Sequence | `[]` | 1 |  |
| `workflow_engine.max_more_workflows_per_org` | Int | `10000` | 2 |  |
| `workflow_engine.max_workflows_per_org` | Int | `1000` | 2 |  |
| `workflow_engine.num_cohorts` | Int | `1` | 1 |  |
| `workflow_engine.schedule.min_cohort_scheduling_age_seconds` | Int | `50` | 1 |  |

### FLAG_MODIFIABLE_BOOL (1)

| Option | Type | Default | Usages | DISK |
|--------|------|---------|--------|------|
| `seer.similarity.token_count_metrics_enabled` | Bool | `True` | 1 |  |

## Move to Django settings (48 options)

Per-environment configuration that should become regular Django settings.
These are `FLAG_NOSTORE` (never written to DB), `FLAG_REQUIRED` (system fundamentals),
or `FLAG_PRIORITIZE_DISK` options that are static per deployment.

### FLAG_NOSTORE (31)

| Option | Type | Default | Usages | DISK |
|--------|------|---------|--------|------|
| `analytics.backend` | inferred | `"noop"` | 4 |  |
| `analytics.options` | inferred | `{}` | 4 |  |
| `codecov.signing_secret` | inferred | `` | 4 |  |
| `filestore.backend` | inferred | `"filesystem"` | 16 |  |
| `filestore.control.backend` | inferred | `""` | 10 |  |
| `filestore.control.options` | inferred | `{}` | 10 |  |
| `filestore.options` | inferred | `{"location": "/tmp/sentry-file` | 16 |  |
| `filestore.profiles-backend` | inferred | `"filesystem"` | 5 |  |
| `filestore.profiles-options` | inferred | `{"location": "/tmp/sentry-prof` | 5 |  |
| `filestore.relocation-backend` | inferred | `"filesystem"` | 6 |  |
| `filestore.relocation-options` | inferred | `{"location": "/tmp/sentry-relo` | 6 |  |
| `github.client-id` | inferred | `` | 8 |  |
| `github.client-secret` | inferred | `` | 6 |  |
| `github.webhook-secret` | inferred | `` | 6 |  |
| `intercom.app-id` | inferred | `` | 2 |  |
| `mail.backend` | inferred | `"smtp"` | 17 |  |
| `mail.list-namespace` | String | `"localhost"` | 4 |  |
| `marketo.base-url` | inferred | `` | 6 |  |
| `marketo.client-id` | inferred | `` | 5 |  |
| `objectstore.config` | inferred | `{         "base_url": "http://` | 5 |  |
| `redis.clusters` | Dict | `{"default": {"hosts": {0: {"ho` | 28 |  |
| `relay.static_auth` | inferred | `{}` | 5 |  |
| `salesforce.webhook-secret` | inferred | `` | 4 |  |
| `system.base-hostname` | inferred | `os.environ.get("SENTRY_SYSTEM_` | 12 | DISK |
| `system.databases` | Dict | `` | 1 |  |
| `system.logging-format` | inferred | `LoggingFormat.HUMAN` | 10 |  |
| `system.organization-base-hostname` | inferred | `os.environ.get("SENTRY_ORGANIZ` | 9 | DISK |
| `system.organization-url-template` | inferred | `os.environ.get("SENTRY_ORGANIZ` | 10 | DISK |
| `system.region` | inferred | `` | 12 | DISK |
| `system.region-api-url-template` | inferred | `os.environ.get("SENTRY_REGION_` | 12 | DISK |
| `viewer-context.enabled` | Bool | `True` | 10 |  |

### FLAG_REQUIRED (12)

| Option | Type | Default | Usages | DISK |
|--------|------|---------|--------|------|
| `auth.allow-registration` | inferred | `False` | 12 | DISK |
| `beacon.anonymous` | Bool | `` | 7 |  |
| `beacon.record_cpu_ram_usage` | Bool | `` | 5 |  |
| `mail.from` | inferred | `"root@localhost"` | 19 | DISK |
| `mail.host` | inferred | `"127.0.0.1"` | 15 | DISK |
| `mail.password` | inferred | `` | 14 | DISK |
| `mail.port` | inferred | `25` | 14 | DISK |
| `mail.use-ssl` | inferred | `False` | 12 | DISK |
| `mail.use-tls` | inferred | `False` | 15 | DISK |
| `mail.username` | inferred | `` | 14 | DISK |
| `system.admin-email` | inferred | `` | 16 |  |
| `system.url-prefix` | inferred | `os.environ.get("SENTRY_SYSTEM_` | 127 | DISK |

### FLAG_ALLOW_EMPTY + DISK (4)

| Option | Type | Default | Usages | DISK |
|--------|------|---------|--------|------|
| `auth-fly.client-id` | inferred | `` | 4 | DISK |
| `auth-fly.client-secret` | inferred | `` | 2 | DISK |
| `auth-google.client-id` | inferred | `` | 8 | DISK |
| `auth-google.client-secret` | inferred | `` | 6 | DISK |

### DEFAULT_FLAGS (1)

| Option | Type | Default | Usages | DISK |
|--------|------|---------|--------|------|
| `msteams.app-id` | inferred | `` | 5 |  |

## Delete (5 options)

The inventory script found **zero references** to these keys anywhere in sentry
or getsentry (outside the `register()` call itself). They are dead code.

> Before deleting: (1) re-run `rg '<key>' ~/code/sentry ~/code/getsentry` to confirm,
> (2) check the production database for non-default values.

### Unused (SAFE_CANDIDATE) (4)

| Option | Type | Default | Usages | DISK |
|--------|------|---------|--------|------|
| `hybrid_cloud.authentication.use_api_key_replica` | Bool | `False` | 0 |  |
| `seer.explorer-index.rollout` | Float | `0.0` | 0 |  |
| `seer.explorer_index.enable` | Bool | `False` | 0 |  |
| `sentry-apps.webhook.circuit-breaker.dry-run` | Bool | `False` | 0 |  |

### Likely unused (INVESTIGATE) (1)

| Option | Type | Default | Usages | DISK |
|--------|------|---------|--------|------|
| `workflow_engine.scheduler.use_conditional_delete` | Bool | `True` | 0 |  |

## Credentials (secrets management) (29 options)

Options containing secrets (API keys, client secrets, tokens). These should stay
in secrets management (env vars, vault, etc.), not in sentry-options ConfigMaps.

| Option | Type | Default | Usages | DISK |
|--------|------|---------|--------|------|
| `aws-lambda.secret-access-key` | inferred | `` | 3 | DISK |
| `codecov.api-bridge-signing-secret` | inferred | `` | 3 | DISK |
| `codecov.client-secret` | inferred | `` | 4 | DISK |
| `discord.bot-token` | inferred | `` | 5 | DISK |
| `discord.client-secret` | inferred | `` | 4 | DISK |
| `flyio.api-secret` | inferred | `` | 3 |  |
| `github-app.client-secret` | inferred | `` | 10 | DISK |
| `github-app.private-key` | inferred | `""` | 4 |  |
| `github-app.webhook-secret` | inferred | `""` | 8 |  |
| `github-console-sdk-app.client-secret` | inferred | `` | 1 | DISK |
| `github-console-sdk-app.installation-id` | inferred | `""` | 6 |  |
| `github-console-sdk-app.private-key` | inferred | `` | 4 | DISK |
| `github-login.client-secret` | inferred | `""` | 6 | DISK |
| `intercom.sentry-api-secret` | inferred | `""` | 3 |  |
| `laravel.api-secret` | inferred | `` | 2 |  |
| `marketo.client-secret` | inferred | `` | 3 | DISK |
| `msteams.client-secret` | inferred | `` | 4 | DISK |
| `nintendo.api-secret` | inferred | `` | 2 |  |
| `slack-staging.client-secret` | inferred | `` | 3 | DISK |
| `slack-staging.signing-secret` | inferred | `` | 3 | DISK |
| `slack.client-secret` | inferred | `` | 5 | DISK |
| `slack.signing-secret` | inferred | `` | 5 | DISK |
| `slack.verification-token` | inferred | `` | 6 | DISK |
| `sms.twilio-token` | inferred | `""` | 2 | DISK |
| `system.secret-key` | inferred | `` | 19 |  |
| `vercel.client-secret` | inferred | `` | 13 | DISK |
| `vsts-limited.client-secret` | inferred | `` | 4 | DISK |
| `vsts.client-secret` | inferred | `` | 3 | DISK |
| `vsts_new.client-secret` | inferred | `` | 2 | DISK |
