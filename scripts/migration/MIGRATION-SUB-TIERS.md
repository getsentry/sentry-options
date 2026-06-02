# Migration Sub-Tiers

Migration ordering for the 653 `FLAG_AUTOMATOR_MODIFIABLE` options,
incorporating production read volume from Datadog, code usage sites,
type complexity, `FLAG_PRIORITIZE_DISK`, and cross-repo usage.

> 13 options reclassified out: 2 to credentials, 11 to Django settings.
> 8 loop-registered `metric-abuse-quota.*` options included (not in `inventory.csv`).

## Summary

| Tier | Count | DD Reads | Description |
|------|-------|----------|-------------|
| **1: Scaffold** | 228 | 0-900 | <1K reads, 0-2 usages, simple types, no DISK |
| **2: Moderate** | 240 | 0-479,300 | Everything not in Tier 1/3/4 |
| **3: High volume** | 162 | 0-9,897,200 | 1M+ reads, OR 6+ usages, OR DISK+high-usage, OR complex+high-reads |
| **4: Critical** | 23 | 0-68,830,600 | 10M+ reads, OR system/mail/staff critical paths |
| **Total** | **653** | | |

---

## Tier 1: Scaffold (228 options)

Near-zero production read volume. Validate the migration pipeline e2e with zero risk.

Stats: 0 DISK, 0 complex types

| Option | Type | Usages | DISK | DD Reads |
|--------|------|--------|------|----------|
| `performance.traces.pagination.max-timeout` | Float | 2 |  | 900 |
| `billing.usagebuffer.redis.pipeline_size` | inferred | 2 |  | 800 |
| `profiling.profile_metrics.unsampled_profiles.sample_rate` | inferred | 2 |  | 800 |
| `relay.endpoint-fetch-config.enabled` | Bool | 1 |  | 800 |
| `workflow_engine.filter_cross_org_workflows` | Bool | 2 |  | 800 |
| `relay.eap-outcomes.rollout-rate` | Float | 2 |  | 700 |
| `relay.eap-span-outcomes.rollout-rate` | Float | 2 |  | 700 |
| `sentry.send_onboarding_task_metrics` | Bool | 1 |  | 700 |
| `backpressure.high_watermarks.attachments-store` | inferred | 0 |  | 600 |
| `seer.max_num_scanner_autotriggered_per_ten_seconds` | inferred | 2 |  | 600 |
| `issues.group_events.batch_nodestore_enabled` | Bool | 1 |  | 500 |
| `performance.traces.transaction_query_timebuffer_days` | Float | 2 |  | 500 |
| `relay.drop-transaction-attachments` | Bool | 1 |  | 500 |
| `sdk-deprecation.profile-chunk.cocoa.hard` | inferred | 1 |  | 500 |
| `filestore-timeout-seconds` | inferred | 1 |  | 400 |
| `hybridcloud.regionsiloclient.retries` | inferred | 1 |  | 400 |
| `processing.severity-backlog-test.error` | inferred | 1 |  | 400 |
| `processing.severity-backlog-test.timeout` | inferred | 1 |  | 400 |
| `replay.consumer.enable_new_query_caching_system` | Bool | 2 |  | 400 |
| `backpressure.monitoring.interval` | inferred | 1 |  | 300 |
| `getsentry.quotas.run_spike_projection.on_missing` | inferred | 2 |  | 300 |
| `relay.metric-bucket-distribution-encodings` | inferred | 2 |  | 300 |
| `relay.objectstore-attachments.sample-rate` | Float | 2 |  | 300 |
| `relay.sessions-eap.rollout-rate` | Float | 2 |  | 300 |
| `snuba.groupsnooze.user-counts-debounce-seconds` | Int | 2 |  | 300 |
| `backpressure.high_watermarks.post-process-locks` | inferred | 0 |  | 200 |
| `backpressure.high_watermarks.processing-store` | inferred | 0 |  | 200 |
| `relay.invalidation-direct-outside-atomic` | inferred | 2 |  | 200 |
| `relay.metric-bucket-set-encodings` | inferred | 2 |  | 200 |
| `relay.span-normalization.allowed_hosts` | inferred | 2 |  | 200 |
| `workflow_engine.schedule.min_cohort_scheduling_age_seconds` | Int | 1 |  | 200 |
| `api-token-async-flush` | Bool | 2 |  | 100 |
| `backpressure.high_watermarks.processing-locks` | inferred | 0 |  | 100 |
| `backpressure.high_watermarks.processing-store-transactions` | inferred | 0 |  | 100 |
| `billing.usage_service.enabled` | inferred | 2 |  | 100 |
| `billing.usagebuffer.scan_limit` | inferred | 1 |  | 100 |
| `crons.system_incidents.pct_deviation_anomaly_threshold` | inferred | 2 |  | 100 |
| `embeddings-grouping.seer.delete-record-batch-size` | Int | 2 |  | 100 |
| `issue-detection.llm-detection.enabled` | Bool | 1 |  | 100 |
| `options_automator_slack_webhook_enabled` | inferred | 1 |  | 100 |
| `outbox_replication.sentry_apikey.replication_version` | Int | 0 |  | 100 |
| `outbox_replication.sentry_authprovider.replication_version` | Int | 1 |  | 100 |
| `outbox_replication.sentry_projectkey.replication_version` | Int | 0 |  | 100 |
| `outbox_replication.sentry_sentryappinstallation.replication_version` | Int | 0 |  | 100 |
| `outbox_replication.sentry_team.replication_version` | Int | 0 |  | 100 |
| `outbox_replication.sentry_userrole_users.replication_version` | Int | 0 |  | 100 |
| `outcomes_consumer.usage_buffer.recover_orphaned_data.enable` | Bool | 2 |  | 100 |
| `performance.trace.span_with_errors_ok_status.sample_rate` | Float | 1 |  | 100 |
| `replay.endpoints.project_replay_summary.trace_sample_rate_post` | inferred | 1 |  | 100 |
| `seer.supergroups_backfill_lightweight.max_failures_per_batch` | Int | 1 |  | 100 |
| `sentry-apps.legacy-webhook-payload-validation.rate` | Float | 1 |  | 100 |
| `sentry.save-event-attachments.project-per-5-minute-limit` | Int | 1 |  | 100 |
| `symbolicate.symx-logging-rate` | inferred | 1 |  | 100 |
| `workflow_engine.num_cohorts` | Int | 1 |  | 100 |
| `api.project-transfer.rate-limit-overrides` | Int | 2 |  | 0 |
| `aws-lambda.host-region` | inferred | 1 |  | 0 |
| `aws-lambda.node.layer-name` | inferred | 1 |  | 0 |
| `aws-lambda.node.layer-version` | inferred | 1 |  | 0 |
| `aws-lambda.python.layer-name` | inferred | 1 |  | 0 |
| `aws-lambda.python.layer-version` | inferred | 1 |  | 0 |
| `aws-lambda.thread-count` | inferred | 1 |  | 0 |
| `backfill_new_categories.chunk_size` | Int | 2 |  | 0 |
| `backfill_new_categories.lock_ttl` | Int | 1 |  | 0 |
| `backfill_new_categories.prioritize_paid_plans` | Bool | 1 |  | 0 |
| `backfill_new_categories.should_run` | Bool | 2 |  | 0 |
| `billing.usage_service.cutover_date` | inferred | 2 |  | 0 |
| `crons.system_incidents.pct_deviation_incident_threshold` | inferred | 2 |  | 0 |
| `crons.system_incidents.tick_decision_window` | inferred | 2 |  | 0 |
| `deletions.group-hash-metadata.batch-size` | Int | 2 |  | 0 |
| `deletions.group-hashes-batch-size` | Int | 1 |  | 0 |
| `discord.debug-channel` | inferred | 1 |  | 0 |
| `discord.debug-server` | inferred | 1 |  | 0 |
| `dynamic-sampling.prioritise_transactions.num_explicit_large_transactions` | inferred | 2 |  | 0 |
| `dynamic-sampling.prioritise_transactions.num_explicit_small_transactions` | inferred | 2 |  | 0 |
| `dynamic-sampling:sliding_window.size` | inferred | 1 |  | 0 |
| `eap-migration.alerts-transactions-rollback.queries` | inferred | 2 |  | 0 |
| `eap-migration.alerts-transactions-rollforward.queries` | inferred | 2 |  | 0 |
| `eap-migration.alerts-transactions.queries` | inferred | 2 |  | 0 |
| `eap-migration.dashboard-comparison.enable` | inferred | 2 |  | 0 |
| `eap-migration.dashboard-comparison.projects` | inferred | 2 |  | 0 |
| `eap-migration.dashboard-comparison.widget-queries` | inferred | 2 |  | 0 |
| `eap-migration.dashboards-transactions.dashboards` | inferred | 1 |  | 0 |
| `eap-migration.discover-transactions.enable` | inferred | 1 |  | 0 |
| `eap-migration.discover-transactions.organizations` | inferred | 2 |  | 0 |
| `eap-migration.discover-transactions.projects` | inferred | 2 |  | 0 |
| `eap-migration.discover-transactions.queries` | inferred | 2 |  | 0 |
| `eventstream.eap.deletion-enabled` | Bool | 2 |  | 0 |
| `explore.trace-items.values.max` | Int | 2 |  | 0 |
| `explorer.service_map.max_edges` | Int | 2 |  | 0 |
| `features.error.capture_rate` | inferred | 1 |  | 0 |
| `filestore.migration.rollout` | inferred | 1 |  | 0 |
| `getsentry.detect-low-value-spans.enabled` | Bool | 2 |  | 0 |
| `getsentry.detect-low-value-spans.llm-max-tokens` | Int | 1 |  | 0 |
| `getsentry.detect-low-value-spans.llm-processing-timeout` | Int | 1 |  | 0 |
| `getsentry.detect-low-value-spans.llm-reasoning` | String | 1 |  | 0 |
| `getsentry.detect-low-value-spans.llm-request-timeout` | Int | 1 |  | 0 |
| `getsentry.override-sample-rate-for-complete-instance` | inferred | 1 |  | 0 |
| `getsentry.spike-protection.calculate_spike_projections` | inferred | 2 |  | 0 |
| `github-app.fetch-commits.max-compare-commits` | Int | 2 |  | 0 |
| `github-secret-scanning.enable-signature-verification` | Bool | 2 |  | 0 |
| `grouping.config_transition.config_upgrade_sample_rate` | Float | 2 |  | 0 |
| `grouping.merge.remove_stuck_group_redirects` | inferred | 1 |  | 0 |
| `grouping.merge.stuck_group_ids` | inferred | 1 |  | 0 |
| `hybridcloud.webhookpayload.worker_threads` | inferred | 2 |  | 0 |
| `inc-984.end` | inferred | 2 |  | 0 |
| `inc-984.parallel.nodestore.read` | inferred | 1 |  | 0 |
| `inc-984.parallel.nodestore.threads` | inferred | 1 |  | 0 |
| `inc-984.snuba.batches` | inferred | 2 |  | 0 |
| `inc-984.start` | inferred | 2 |  | 0 |
| `insights.span-samples-query.sample-rate` | Float | 1 |  | 0 |
| `integrations.backfill_github_external_actor.gh_api_fetch_interval_s` | Float | 1 |  | 0 |
| `issue-detection.web-vitals-detection.enabled` | Bool | 2 |  | 0 |
| `issues.record-seer-actions-as-activities` | Bool | 2 |  | 0 |
| `issues.severity.seer-timeout` | Float | 2 |  | 0 |
| `on_demand.extended_max_alert_specs` | inferred | 1 |  | 0 |
| `on_demand.extended_max_widget_specs` | inferred | 2 |  | 0 |
| `on_demand.extended_widget_spec_orgs` | inferred | 2 |  | 0 |
| `on_demand.max_widget_cardinality.killswitch` | inferred | 1 |  | 0 |
| `on_demand.max_widget_cardinality.on_query_count` | inferred | 2 |  | 0 |
| `on_demand.max_widget_specs` | inferred | 2 |  | 0 |
| `outbox_replication.accounts_thirdpartyaccount.replication_version` | Int | 0 |  | 0 |
| `outbox_replication.auth_authenticator.replication_version` | Int | 0 |  | 0 |
| `outbox_replication.auth_user.replication_version` | Int | 0 |  | 0 |
| `outbox_replication.sentry_apitoken.replication_version` | Int | 0 |  | 0 |
| `outbox_replication.sentry_authidentity.replication_version` | Int | 0 |  | 0 |
| `outbox_replication.sentry_externalactor.replication_version` | Int | 0 |  | 0 |
| `outbox_replication.sentry_organization.replication_version` | Int | 1 |  | 0 |
| `outbox_replication.sentry_organizationavatar.replication_version` | Int | 0 |  | 0 |
| `outbox_replication.sentry_organizationintegration.replication_version` | Int | 0 |  | 0 |
| `outbox_replication.sentry_organizationmember.replication_version` | Int | 0 |  | 0 |
| `outbox_replication.sentry_organizationmember_teams.replication_version` | Int | 0 |  | 0 |
| `outbox_replication.sentry_organizationslugreservation.replication_version` | Int | 0 |  | 0 |
| `outbox_replication.sentry_orgauthtoken.replication_version` | Int | 0 |  | 0 |
| `outbox_replication.sentry_sentryappinstallationtoken.replication_version` | Int | 0 |  | 0 |
| `outbox_replication.sentry_useremail.replication_version` | Int | 0 |  | 0 |
| `outbox_replication.sentry_userpermission.replication_version` | Int | 0 |  | 0 |
| `outbox_replication.sentry_userrole.replication_version` | Int | 0 |  | 0 |
| `outcomes_consumer.usage_buffer.percent.rollout` | inferred | 1 |  | 0 |
| `outcomes_consumer.usage_buffer.recover_orphaned_data.limit` | Int | 2 |  | 0 |
| `performance.extrapolation.confidence.z-score` | Float | 1 |  | 0 |
| `performance.spans-tags-key.max` | Int | 1 |  | 0 |
| `performance.spans-tags-values.max` | Int | 1 |  | 0 |
| `performance.traces.query_timestamp_projects` | Bool | 1 |  | 0 |
| `performance.traces.span_query_timebuffer_hours` | Float | 1 |  | 0 |
| `performance.traces.trace-explorer-skip-recent-seconds` | Int | 1 |  | 0 |
| `profiling.continuous-profiling.chunks-query.size` | Int | 1 |  | 0 |
| `profiling.continuous-profiling.chunks-set.size` | Int | 2 |  | 0 |
| `profiling.flamegraph.profile-set.size` | Int | 2 |  | 0 |
| `profiling.flamegraph.query.initial_chunk_delta.hours` | Int | 1 |  | 0 |
| `profiling.flamegraph.query.max_delta.hours` | Int | 1 |  | 0 |
| `profiling.flamegraph.query.multiplier` | Int | 1 |  | 0 |
| `provision_organization.override.rate` | Float | 2 |  | 0 |
| `relay.kafka.span-v2.sample-rate` | Float | 1 |  | 0 |
| `relay.projectconfigs.migration.rollout` | inferred | 1 |  | 0 |
| `relay.quotas.migration.rollout` | inferred | 2 |  | 0 |
| `release-health.monitor-release-adoption-jitter-seconds` | Int | 2 |  | 0 |
| `release-health.use-org-and-project-filter` | Bool | 2 |  | 0 |
| `relocation.autopause.saas-to-saas` | inferred | 2 |  | 0 |
| `relocation.autopause.self-hosted` | inferred | 2 |  | 0 |
| `relocation.daily-limit.large` | inferred | 0 |  | 0 |
| `relocation.daily-limit.medium` | inferred | 1 |  | 0 |
| `replay.endpoints.project_replay_summary.trace_sample_rate_get` | inferred | 1 |  | 0 |
| `reprocessing2.drop-delete-old-primary-hash` | inferred | 2 |  | 0 |
| `sdk-deprecation.profile-chunk.cocoa` | inferred | 2 |  | 0 |
| `sdk-deprecation.profile-chunk.cocoa.reject` | inferred | 1 |  | 0 |
| `sdk-deprecation.profile-chunk.python` | inferred | 2 |  | 0 |
| `sdk-deprecation.profile.cocoa.reject` | inferred | 1 |  | 0 |
| `secret-scanning.github.enable-signature-verification` | Bool | 2 |  | 0 |
| `seer.api.use-shared-secret` | inferred | 1 |  | 0 |
| `seer.explorer.context-engine-rollout` | Float | 1 |  | 0 |
| `seer.max_num_autofix_autotriggered_per_hour` | inferred | 2 |  | 0 |
| `seer.night_shift.enable` | Bool | 2 |  | 0 |
| `seer.similarity-embeddings-delete-by-hash-killswitch.enabled` | Bool | 1 |  | 0 |
| `seer.similarity.ingest.store_hybrid_fingerprint_non_matches` | Bool | 1 |  | 0 |
| `seer.supergroups_backfill_lightweight.batch_size` | Int | 2 |  | 0 |
| `seer.supergroups_backfill_lightweight.inter_batch_delay_s` | Int | 1 |  | 0 |
| `seer.supergroups_backfill_lightweight.killswitch` | Bool | 2 |  | 0 |
| `sentry-metrics.10s-granularity` | inferred | 2 |  | 0 |
| `sentry-metrics.cardinality-limiter.limits.custom.per-org` | inferred | 0 |  | 0 |
| `sentry-metrics.cardinality-limiter.limits.generic-metrics.per-org` | inferred | 0 |  | 0 |
| `sentry-metrics.cardinality-limiter.limits.profiles.per-org` | inferred | 0 |  | 0 |
| `sentry-metrics.cardinality-limiter.limits.sessions.per-org` | inferred | 0 |  | 0 |
| `sentry-metrics.cardinality-limiter.limits.spans.per-org` | inferred | 0 |  | 0 |
| `sentry-metrics.cardinality-limiter.limits.transactions.per-org` | inferred | 0 |  | 0 |
| `sentry-metrics.synchronized-rebalance-delay` | inferred | 1 |  | 0 |
| `sentry-metrics.writes-limiter.limits.custom.global` | inferred | 0 |  | 0 |
| `sentry-metrics.writes-limiter.limits.custom.per-org` | inferred | 0 |  | 0 |
| `sentry-metrics.writes-limiter.limits.generic-metrics.global` | inferred | 1 |  | 0 |
| `sentry-metrics.writes-limiter.limits.generic-metrics.per-org` | inferred | 1 |  | 0 |
| `sentry-metrics.writes-limiter.limits.performance.global` | inferred | 1 |  | 0 |
| `sentry-metrics.writes-limiter.limits.performance.per-org` | inferred | 1 |  | 0 |
| `sentry-metrics.writes-limiter.limits.sessions.global` | inferred | 0 |  | 0 |
| `sentry-metrics.writes-limiter.limits.sessions.per-org` | inferred | 0 |  | 0 |
| `sentry-metrics.writes-limiter.limits.spans.global` | inferred | 1 |  | 0 |
| `sentry-metrics.writes-limiter.limits.spans.per-org` | inferred | 1 |  | 0 |
| `sentry-metrics.writes-limiter.limits.transactions.global` | inferred | 1 |  | 0 |
| `sentry-metrics.writes-limiter.limits.transactions.per-org` | inferred | 1 |  | 0 |
| `sentry.save-event-attachments.project-per-sec-limit` | Int | 1 |  | 0 |
| `similarity.new_project_seer_grouping.enabled` | inferred | 2 |  | 0 |
| `slack.debug-channel` | inferred | 1 |  | 0 |
| `slack.debug-workspace` | inferred | 1 |  | 0 |
| `slack.log-unfurl-payload` | inferred | 1 |  | 0 |
| `snuba.search.min-pre-snuba-candidates` | inferred | 1 |  | 0 |
| `snuba.search.pre-snuba-candidates-optimizer` | Bool | 1 |  | 0 |
| `snuba.search.recommended.event-volume-weight` | inferred | 1 |  | 0 |
| `snuba.search.recommended.recency-weight` | inferred | 1 |  | 0 |
| `snuba.search.recommended.severity-weight` | inferred | 1 |  | 0 |
| `snuba.search.recommended.spike-weight` | inferred | 1 |  | 0 |
| `snuba.search.recommended.user-impact-weight` | inferred | 1 |  | 0 |
| `spans.buffer.evalsha-latency-threshold` | Int | 1 |  | 0 |
| `statistical_detectors.throughput.threshold.transactions` | Int | 1 |  | 0 |
| `store.allow-s4s-ddm-sample-rate` | inferred | 1 |  | 0 |
| `symbolicate.symx-os-description-list` | inferred | 1 |  | 0 |
| `symbolicator.sourcemaps-bundle-index-refresh-sample-rate` | inferred | 1 |  | 0 |
| `tempest.crashes-timeout` | inferred | 2 |  | 0 |
| `tempest.latest-id-timeout` | inferred | 2 |  | 0 |
| `tempest.lock-buffer-seconds` | inferred | 2 |  | 0 |
| `tempest.poll-limit` | inferred | 1 |  | 0 |
| `tempest.task-deadline-seconds` | inferred | 2 |  | 0 |
| `uptime.automatic-subscription-creation` | Bool | 2 |  | 0 |
| `uptime.update-checker-script-interval-seconds` | inferred | 2 |  | 0 |
| `vercel.integration-slug` | inferred | 1 |  | 0 |
| `vercel.invoice-notpaid.disable-downgrade` | inferred | 2 |  | 0 |
| `visibility.tag-key-max-date-range.days` | inferred | 2 |  | 0 |
| `visibility.tag-key-sample-size` | inferred | 1 |  | 0 |
| `vsts.social-auth-migration` | Bool | 1 |  | 0 |
| `workflow_engine.max_more_workflows_per_org` | Int | 2 |  | 0 |
| `workflow_engine.max_workflows_per_org` | Int | 2 |  | 0 |

## Tier 2: Moderate (240 options)

The main body of the migration. Moderate production reads, manageable code refs. Migrate in batches by namespace.

Stats: 77 DISK, 48 complex types

| Option | Type | Usages | DISK | DD Reads |
|--------|------|--------|------|----------|
| `dynamic-sampling.per_org.rollout-rate` | Float | 3 |  | 479,300 |
| `sentry-metrics.indexer.read-new-cache-namespace` | inferred | 3 |  | 416,300 |
| `replay.consumer.msgspec_recording_parser` | Bool | 1 |  | 379,900 |
| `uptime.use-detectors-by-data-source-cache` | Bool | 1 |  | 355,700 |
| `getsentry.rate-limit.project-replays` | Int | 1 | DISK | 325,000 |
| `getsentry.rate-limit.org-transactions` | Int | 1 | DISK | 323,300 |
| `getsentry.rate-limit.org-errors` | Int | 1 | DISK | 319,400 |
| `getsentry.rate-limit.project-profiles` | Int | 1 | DISK | 318,800 |
| `getsentry.rate-limit.project-logs` | Int | 1 | DISK | 317,500 |
| `getsentry.rate-limit.project-profile.duration-ui` | Int | 1 | DISK | 317,200 |
| `getsentry.rate-limit.project-trace-metrics` | Int | 1 | DISK | 317,200 |
| `getsentry.rate-limit.org-profiles` | Int | 1 | DISK | 316,600 |
| `getsentry.rate-limit.org-replays` | Int | 1 | DISK | 315,800 |
| `metric-abuse-quota.organization.transactions` | Int | 0 | DISK | 315,400 |
| `getsentry.rate-limit.org-profile.duration-ui` | Int | 1 | DISK | 314,300 |
| `getsentry.rate-limit.org-spans` | Int | 1 | DISK | 314,300 |
| `getsentry.rate-limit.project-feedback` | Int | 1 | DISK | 313,400 |
| `getsentry.rate-limit.project-feedback-sustained.limit` | Int | 1 | DISK | 313,000 |
| `metric-abuse-quota.organization` | Int | 0 | DISK | 313,000 |
| `getsentry.rate-limit.project-feedback-sustained.window` | Int | 1 | DISK | 312,800 |
| `metric-abuse-quota.project` | Int | 0 | DISK | 312,700 |
| `metric-abuse-quota.organization.spans` | Int | 0 | DISK | 312,600 |
| `getsentry.rate-limit.org-trace-metrics` | Int | 1 | DISK | 311,200 |
| `getsentry.rate-limit.org-profile.duration` | Int | 1 | DISK | 311,000 |
| `metric-abuse-quota.project.spans` | Int | 0 | DISK | 309,800 |
| `metric-abuse-quota.organization.sessions` | Int | 0 | DISK | 309,800 |
| `getsentry.rate-limit.project-profile.duration` | Int | 1 | DISK | 308,200 |
| `metric-abuse-quota.project.sessions` | Int | 0 | DISK | 308,100 |
| `getsentry.rate-limit.project-spans` | Int | 1 | DISK | 307,700 |
| `hybridcloud.rpc.method_timeout_overrides` | inferred | 3 |  | 307,700 |
| `metric-abuse-quota.project.transactions` | Int | 0 | DISK | 307,300 |
| `getsentry.rate-limit.project-transactions` | Int | 1 | DISK | 307,200 |
| `project-abuse-quota.window` | Int | 2 | DISK | 306,800 |
| `getsentry.rate-limit.org-feedback` | Int | 1 | DISK | 305,000 |
| `hybridcloud.rpc.method_retry_overrides` | inferred | 2 |  | 303,900 |
| `hybrid_cloud.rpc.disabled-service-methods` | inferred | 2 |  | 303,700 |
| `getsentry.rate-limit.org-logs` | Int | 1 | DISK | 302,300 |
| `hybridcloud.rpc.retries` | inferred | 2 |  | 298,800 |
| `consumer.join.profiling.rate` | Float | 1 |  | 293,600 |
| `spans.process-spans.profiling.rate` | Float | 1 | DISK | 289,100 |
| `sentry-metrics.indexer.generic-metrics.schema-validation-rules` | inferred | 1 |  | 201,700 |
| `sentry-metrics.indexer.release-health.schema-validation-rules` | inferred | 1 |  | 182,300 |
| `spans.buffer.redis-ttl` | Int | 2 | DISK | 178,500 |
| `sentry-apps.webhook.timeout.sec` | inferred | 3 |  | 129,900 |
| `sentry-apps.disabled-enforcement` | Bool | 4 |  | 128,700 |
| `sentry-apps.webhook.hard-timeout.sec` | inferred | 2 |  | 127,200 |
| `sentry-apps.disable-paranoia` | Bool | 4 |  | 120,200 |
| `hybridcloud.webhookpayload.push_drain_trigger` | inferred | 2 |  | 111,100 |
| `subscriptions-query.sample-rate` | inferred | 1 |  | 107,700 |
| `span-metrics-extraction-addons-enabled` | inferred | 2 |  | 99,100 |
| `span-metrics-extraction-addons-orgs-denylist` | Sequence | 2 |  | 98,600 |
| `dynamic-sampling.per_org.metrics-sample-rate` | Float | 2 |  | 91,200 |
| `profiling.profile_metrics.unsampled_profiles.enabled` | Bool | 4 |  | 90,700 |
| `sdk-deprecation.profile-chunk.python.hard` | inferred | 1 |  | 84,900 |
| `hybridcloud.apigateway.use_pooling.rate` | Float | 2 |  | 61,500 |
| `apigateway.proxy.circuit-breaker.enabled` | Bool | 1 |  | 57,500 |
| `apigateway.proxy.timeout` | Int | 1 |  | 56,300 |
| `dynamic-sampling.per_org.killswitch` | inferred | 3 |  | 47,300 |
| `integrations.slo.integration-id-tag-enabled` | Bool | 1 |  | 47,300 |
| `getsentry.override-sample-rate-for-instance` | inferred | 1 |  | 46,400 |
| `spans.buffer.max-spans-per-evalsha` | Int | 2 | DISK | 38,600 |
| `spans.buffer.pipeline-batch-size` | Int | 2 | DISK | 37,300 |
| `spans.buffer.max-memory-percentage` | Float | 2 | DISK | 37,100 |
| `issues.occurrence-consumer.rate-limit.quota` | Dict | 2 |  | 36,900 |
| `standalone-span-discard-transaction` | inferred | 2 |  | 35,500 |
| `spans.buffer.compression.level` | Int | 2 | DISK | 35,300 |
| `spans.buffer.flusher.backpressure-seconds` | inferred | 2 | DISK | 34,900 |
| `issues.occurrence-consumer.rate-limit.enabled` | Bool | 2 |  | 33,900 |
| `release-health.disable-release-last-seen-update` | Bool | 2 |  | 28,000 |
| `hybridcloud.webhookpayload.skip_on_failure_providers` | Sequence | 1 |  | 24,200 |
| `github.webhook.mailbox-bucketing.enabled` | inferred | 2 |  | 21,200 |
| `continuous-profiling-beta` | inferred | 5 |  | 19,700 |
| `sentry-metrics.indexer.write-new-cache-namespace` | inferred | 3 |  | 19,700 |
| `sentry.scm.stream.rollout` | Float | 1 | DISK | 19,700 |
| `on_demand_metrics.cache_should_use_on_demand` | inferred | 3 |  | 18,200 |
| `cleanup.abort_execution` | Bool | 1 |  | 15,900 |
| `sentry.search.events.project.check_event` | Float | 1 |  | 14,500 |
| `data-forwarding.project-cache-ttl` | Int | 1 |  | 14,000 |
| `apigateway.proxy.circuit-breaker.enforce` | Bool | 1 |  | 13,000 |
| `post_process.get-autoassign-owners` | Sequence | 2 |  | 13,000 |
| `delayed_processing.batch_size` | inferred | 3 |  | 12,900 |
| `sdk_http2_experiment.enabled` | Bool | 1 |  | 12,600 |
| `taskworker.producer.max_futures` | Int | 1 |  | 12,500 |
| `dynamic-sampling.config.killswitch` | inferred | 1 |  | 10,500 |
| `indexed-spans-extraction-orgs-denylist` | Sequence | 2 |  | 10,200 |
| `standalone-span-discard-transaction-project-allowlist` | Sequence | 2 |  | 10,100 |
| `indexed-spans-extraction-enabled` | inferred | 2 |  | 9,900 |
| `replay.replay-video.disabled` | Bool | 2 | DISK | 9,600 |
| `seer.similarity.circuit-breaker-config` | Dict | 2 |  | 9,400 |
| `relay.drop-transaction-metrics` | inferred | 2 |  | 9,100 |
| `sentry-metrics.releasehealth.abnormal-mechanism-extraction-rate` | inferred | 3 |  | 9,000 |
| `workflow_engine.group.type_id.open_periods_type_denylist` | Sequence | 1 |  | 8,900 |
| `replay.replay-video.slug-denylist` | Sequence | 2 | DISK | 8,600 |
| `on_demand.extended_alert_spec_orgs` | inferred | 1 |  | 8,500 |
| `hybridcloud.integrationproxy.retries` | inferred | 1 |  | 8,200 |
| `on_demand.max_alert_specs` | inferred | 2 |  | 8,200 |
| `backpressure.checking.enabled` | inferred | 3 |  | 8,000 |
| `backpressure.monitoring.enabled` | inferred | 4 |  | 7,500 |
| `snuba.search.max-pre-snuba-candidates` | inferred | 2 |  | 6,600 |
| `statistical_detectors.throughput.threshold.functions` | Int | 1 |  | 5,900 |
| `issues.severity.skip-seer-requests` | Sequence | 4 |  | 5,700 |
| `snuba.search.chunk-growth-rate` | inferred | 1 |  | 5,300 |
| `snuba.search.max-chunk-size` | inferred | 1 |  | 5,300 |
| `seer.global-killswitch.enabled` | Bool | 2 |  | 5,200 |
| `seer.similarity.token_count_metrics_enabled` | Bool | 1 |  | 5,000 |
| `seer.similarity.grouping-ingest-retries` | Int | 2 |  | 4,900 |
| `snuba.search.max-total-chunk-time-seconds` | inferred | 1 |  | 4,900 |
| `mail.timeout` | Int | 2 | DISK | 4,700 |
| `on_demand_metrics.widgets.use_stateful_extraction` | inferred | 2 | DISK | 4,700 |
| `seer.similarity-killswitch.enabled` | Bool | 2 |  | 4,700 |
| `seer.similarity.grouping_killswitch_projects` | Sequence | 3 |  | 4,700 |
| `workflow_engine.associate_error_detectors` | Bool | 2 |  | 4,700 |
| `seer.similarity.global-rate-limit` | Dict | 1 |  | 4,500 |
| `insights-query-date-range-limit.enable` | Bool | 2 |  | 4,400 |
| `seer.similarity.max_token_count` | Int | 3 |  | 4,400 |
| `seer.similarity.ingest.num_matches_to_request` | Int | 2 |  | 4,300 |
| `on_demand.update_on_demand_modified` | inferred | 1 |  | 4,200 |
| `store.load-shed-group-creation-projects` | Any | 3 |  | 4,200 |
| `crons.dispatch_incident_occurrences_to_consumer` | inferred | 2 |  | 4,100 |
| `seer.similarity.per-project-rate-limit` | Dict | 1 |  | 4,100 |
| `hybrid_cloud.authentication.disabled_organization_shards` | Sequence | 3 |  | 4,000 |
| `seer.similarity-embeddings-killswitch.enabled` | Bool | 2 |  | 3,800 |
| `crons.system_incidents.use_decisions` | inferred | 4 |  | 3,700 |
| `seer.similarity.grouping-ingest-timeout` | Int | 2 |  | 3,700 |
| `consumer.shared_memory_spawn_process` | Bool | 1 |  | 3,500 |
| `organization.default-owner-id-cache-ttl` | Int | 1 |  | 3,400 |
| `relocation.selectable-regions` | inferred | 1 |  | 3,200 |
| `api.deprecation.brownout-cron` | String | 2 |  | 3,000 |
| `chunk-upload.no-compression` | inferred | 2 |  | 3,000 |
| `performance.traces.spans_extraction_date` | Int | 1 |  | 2,900 |
| `api.deprecation.brownout-duration` | Int | 2 |  | 2,800 |
| `billing.usagebuffer.unified_pipeline.rollout` | inferred | 3 |  | 2,800 |
| `releases.no_snuba_for_release_creation` | Bool | 2 |  | 2,700 |
| `hybrid_cloud.disable_relative_upload_urls` | inferred | 2 |  | 2,600 |
| `eventstore.adjacent_event_ids_use_snql` | Bool | 1 |  | 2,300 |
| `sentry-metrics.writes-limiter.limits.releasehealth.global` | inferred | 1 |  | 2,000 |
| `billing.add-billing-metric-usage-admin.organizations` | Sequence | 1 |  | 1,800 |
| `snuba.search.hits-sample-size` | inferred | 2 |  | 1,800 |
| `billing.usagebuffer.unified_pipeline.chunk_size` | inferred | 2 |  | 1,700 |
| `apigateway.cell_resolver.enabled` | Bool | 4 |  | 1,600 |
| `backfill_new_categories.org_ids` | Sequence | 3 |  | 1,600 |
| `hybrid_cloud.authentication.disabled_user_shards` | Sequence | 3 |  | 1,400 |
| `performance.traces.check_span_extraction_date` | Bool | 1 |  | 1,400 |
| `demo-org-ids` | inferred | 1 |  | 1,300 |
| `sentry-metrics.indexer.disable-memcache-replenish-rollout` | inferred | 1 |  | 1,300 |
| `hybrid_cloud.disable_tombstone_cleanup` | inferred | 1 |  | 1,200 |
| `performance.traces.pagination.max-iterations` | Int | 2 |  | 1,100 |
| `relay.span-usage-metric` | inferred | 2 |  | 1,100 |
| `performance.traces.pagination.query-limit` | Int | 2 |  | 1,000 |
| `sentry-metrics.writes-limiter.limits.releasehealth.per-org` | inferred | 1 |  | 1,000 |
| `consumer.dump_stacktrace_on_shutdown` | Sequence | 1 |  | 700 |
| `issues.severity.seer-global-rate-limit` | Any | 1 |  | 600 |
| `seer.organizations.force-config-reminder` | Sequence | 2 |  | 600 |
| `issues.severity.seer-circuit-breaker-passthrough-limit` | Dict | 1 |  | 500 |
| `dashboards.prebuilt-dashboard-ids` | Sequence | 2 |  | 400 |
| `issues.severity.seer-project-rate-limit` | Any | 1 |  | 400 |
| `feedback.organizations.slug-denylist` | Sequence | 2 |  | 300 |
| `issue-detection.llm-detection.traces-per-invocation` | Dict | 2 |  | 300 |
| `api.organization.disable-last-deploys` | Sequence | 3 |  | 200 |
| `backpressure.status_ttl` | inferred | 3 |  | 200 |
| `consumer.verbose_multiprocessing_logs` | Sequence | 1 |  | 200 |
| `explore.trace-items.keys.max` | Int | 5 |  | 200 |
| `notifications.platform-rollout.general-access` | Dict | 1 |  | 200 |
| `statistical_detectors.query.batch_size` | Int | 1 | DISK | 200 |
| `dynamic-sampling.prioritise_transactions.rebalance_intensity` | inferred | 3 |  | 100 |
| `getsentry.options-dual-read-test` | Bool | 1 | DISK | 100 |
| `notifications.platform-rollout.early-adopter` | Dict | 1 |  | 100 |
| `on_demand.max_widget_cardinality.count` | inferred | 4 |  | 100 |
| `profiling.profile_metrics.unsampled_profiles.platforms` | Sequence | 2 |  | 100 |
| `seer.code-review.excluded-pr-author-logins` | Sequence | 2 |  | 100 |
| `aws-lambda.account-number` | inferred | 4 |  | 0 |
| `aws-lambda.cloudformation-url` | inferred | 4 |  | 0 |
| `backfill_new_categories.categories` | Sequence | 3 |  | 0 |
| `billing.create_invoices.tasks_per_second` | inferred | 3 |  | 0 |
| `crons.system_incidents.collect_metrics` | inferred | 3 |  | 0 |
| `delayed_workflow.rollout` | Bool | 3 |  | 0 |
| `devtoolbar.analytics.enabled` | Bool | 2 | DISK | 0 |
| `eap-migration.dashboards-transactions.organizations` | inferred | 3 |  | 0 |
| `explorer.context_engine_indexing.enable` | Bool | 4 |  | 0 |
| `explorer.service_map.max_segments` | Int | 1 | DISK | 0 |
| `explorer.service_map.parent_span_batch_size` | Int | 1 | DISK | 0 |
| `feedback.filter_garbage_messages` | Bool | 2 | DISK | 0 |
| `feedback.message.max-size` | Int | 3 |  | 0 |
| `flags:options-audit-log-is-enabled` | Bool | 2 | DISK | 0 |
| `flags:options-audit-log-organization-id` | Int | 2 | DISK | 0 |
| `getsentry.detect-low-value-spans.internal-org-slugs` | Sequence | 2 |  | 0 |
| `getsentry.instance-sample-rate-per-project` | Dict | 1 |  | 0 |
| `getsentry.rate-limit.org-metric.seconds` | Int | 1 | DISK | 0 |
| `getsentry.rate-limit.project-metric.seconds` | Int | 1 | DISK | 0 |
| `github-app.rate-limit-sensitive-orgs` | Sequence | 2 |  | 0 |
| `github-enterprise-app.allowed-hosts-legacy-webhooks` | Sequence | 2 |  | 0 |
| `github-login.api-domain` | inferred | 1 | DISK | 0 |
| `github-login.base-domain` | inferred | 1 | DISK | 0 |
| `github-login.extended-permissions` | Sequence | 2 | DISK | 0 |
| `github-login.organization` | inferred | 1 | DISK | 0 |
| `github-login.require-verified-email` | Bool | 1 | DISK | 0 |
| `hybrid_cloud.audit_log_event_id_invalid_pass_list` | Sequence | 2 |  | 0 |
| `inc-984.projects` | inferred | 3 |  | 0 |
| `issue-detection.web-vitals-detection.projects-allowlist` | Sequence | 1 |  | 0 |
| `metric_alerts.extended_max_subscriptions` | inferred | 5 |  | 0 |
| `metric_alerts.extended_max_subscriptions_orgs` | inferred | 5 |  | 0 |
| `notifications.platform-rollout.is-sentry` | Dict | 2 |  | 0 |
| `notifications.platform.killswitch.sources` | Sequence | 1 |  | 0 |
| `objectstore.enable_for.attachments` | inferred | 4 |  | 0 |
| `on_demand_metrics.check_widgets.query.batch_size` | Int | 2 | DISK | 0 |
| `on_demand_metrics.check_widgets.query.total_batches` | inferred | 2 | DISK | 0 |
| `on_demand_metrics.check_widgets.rollout` | Float | 2 | DISK | 0 |
| `organization-abuse-quota.metric-bucket-limit` | Int | 1 | DISK | 0 |
| `outcomes_consumer.usage_buffer.allowlist.rollout` | Sequence | 1 |  | 0 |
| `pagerduty.app-id` | inferred | 5 |  | 0 |
| `project-abuse-quota.span-limit` | Int | 1 | DISK | 0 |
| `project-abuse-quota.transaction-limit` | Int | 1 | DISK | 0 |
| `provision_organization.override.mapping` | Dict | 2 |  | 0 |
| `recovery.disallow-new-enrollment` | Bool | 0 | DISK | 0 |
| `releasefile.cache-path` | String | 2 | DISK | 0 |
| `relocation.autopause` | inferred | 3 |  | 0 |
| `relocation.daily-limit.small` | inferred | 3 |  | 0 |
| `repository.auto-link-by-name-dry-run` | Bool | 3 |  | 0 |
| `seer.explorer_index.killswitch.enable` | Bool | 4 |  | 0 |
| `seer.night_shift.issues_per_org` | inferred | 5 |  | 0 |
| `sentry-apps.hard-delete` | Bool | 5 |  | 0 |
| `sentry.demo_mode.sync_debug_artifacts.enable` | Bool | 1 | DISK | 0 |
| `sentry.demo_mode.sync_debug_artifacts.source_org_id` | Int | 1 | DISK | 0 |
| `sms.disallow-new-enrollment` | Bool | 3 |  | 0 |
| `sms.twilio-number` | inferred | 2 | DISK | 0 |
| `snuba.search.recommended.group-type-boost` | Dict | 2 |  | 0 |
| `snuba.tagstore.cache-tagkeys-rate` | inferred | 1 | DISK | 0 |
| `statistical_detectors.enable` | inferred | 2 | DISK | 0 |
| `statistical_detectors.query.functions.timeseries_days` | Int | 1 | DISK | 0 |
| `statistical_detectors.query.transactions.timeseries_days` | Int | 1 | DISK | 0 |
| `statistical_detectors.ratelimit.ema` | Int | 2 | DISK | 0 |
| `symbolserver.options` | inferred | 1 | DISK | 0 |
| `tempest.tempest-ips-api-response` | Sequence | 2 |  | 0 |
| `totp.disallow-new-enrollment` | Bool | 1 | DISK | 0 |
| `u2f.app-id` | inferred | 1 | DISK | 0 |
| `u2f.disallow-new-enrollment` | Bool | 1 | DISK | 0 |
| `uptime.create-issues` | Bool | 3 |  | 0 |
| `uptime.restrict-issue-creation-by-hosting-provider-id` | Sequence | 2 |  | 0 |
| `uptime.uptime-ips-api-response` | Sequence | 2 |  | 0 |
| `vsts.consent-prompt` | inferred | 3 |  | 0 |

## Tier 3: High volume (162 options)

Significant production read volume or multiple risk factors. Careful testing required.

Stats: 46 DISK, 34 complex types

| Option | Type | Usages | DISK | DD Reads |
|--------|------|--------|------|----------|
| `sentry-metrics.drop-percentiles.per-use-case` | inferred | 3 |  | 9,897,200 |
| `backpressure.checking.interval` | inferred | 2 |  | 9,632,900 |
| `store.load-shed-pipeline-projects` | Any | 3 |  | 9,175,500 |
| `store.load-shed-parsed-pipeline-projects` | Any | 2 |  | 9,162,500 |
| `store.load-shed-save-event-projects` | Any | 2 |  | 8,806,300 |
| `nodestore.set-subkeys.enable-set-cache-item` | inferred | 2 |  | 8,661,600 |
| `eventstream.eap_forwarding_rate` | inferred | 3 |  | 8,641,800 |
| `eventstream:kafka-headers` | inferred | 1 |  | 8,641,300 |
| `flagpole.missing_features_logging_rate` | inferred | 1 |  | 7,748,800 |
| `sentry:skip-record-onboarding-tasks-if-complete` | Bool | 1 |  | 7,578,700 |
| `spans.buffer.max-segment-bytes` | Int | 4 | DISK | 7,426,400 |
| `spans.process-segments.skip-enrichment-projects` | Sequence | 2 |  | 7,401,700 |
| `spans.process-segments.dedupe-filter-enable` | inferred | 2 | DISK | 7,398,700 |
| `spans.process-segments.dedupe-ttl` | Int | 2 | DISK | 7,379,800 |
| `spans.process-segments.consumer.enable` | inferred | 2 | DISK | 7,349,700 |
| `spans.process-segments.detect-performance-problems.enable` | inferred | 2 | DISK | 7,315,900 |
| `spans.process-segments.drop-segments` | Sequence | 3 |  | 7,278,000 |
| `performance.issues.render_blocking_assets.fcp_ratio_threshold` | inferred | 2 |  | 7,112,700 |
| `performance.issues.sql_injection.query_value_length_threshold` | inferred | 1 |  | 7,104,700 |
| `performance.issues.n_plus_one_api_calls.total_duration` | inferred | 2 |  | 7,104,300 |
| `performance.issues.n_plus_one_db.count_threshold` | inferred | 2 |  | 7,102,400 |
| `performance.issues.n_plus_one_api_calls.problem-creation` | inferred | 2 |  | 7,101,500 |
| `performance.issues.render_blocking_assets.fcp_minimum_threshold` | inferred | 1 |  | 7,098,200 |
| `performance.issues.slow_db_query.duration_threshold` | inferred | 2 |  | 7,097,400 |
| `performance.issues.consecutive_db.min_time_saved_threshold` | inferred | 2 |  | 7,095,500 |
| `performance.issues.uncompressed_asset.size_threshold` | inferred | 2 |  | 7,091,900 |
| `performance.issues.slow_db_query.problem-creation` | inferred | 4 |  | 7,090,300 |
| `performance.issues.http_overhead.problem-creation` | inferred | 1 |  | 7,089,100 |
| `performance.issues.consecutive_http.consecutive_count_threshold` | inferred | 1 |  | 7,080,500 |
| `performance.issues.large_http_payload.size_threshold` | inferred | 2 |  | 7,075,100 |
| `performance.issues.m_n_plus_one_db.problem-creation` | inferred | 1 |  | 7,070,900 |
| `performance.issues.all.problem-detection` | inferred | 6 |  | 7,070,300 |
| `performance.issues.consecutive_http.max_duration_between_spans` | inferred | 1 |  | 7,066,300 |
| `performance.issues.consecutive_http.min_time_saved_threshold` | inferred | 2 |  | 7,064,400 |
| `performance.issues.uncompressed_asset.duration_threshold` | inferred | 2 |  | 7,062,500 |
| `performance.issues.consecutive_db.problem-creation` | inferred | 1 |  | 7,062,100 |
| `performance.issues.large_http_payload.filtered_paths` | inferred | 1 |  | 7,061,000 |
| `performance.issues.http_overhead.http_request_delay_threshold` | inferred | 1 |  | 7,060,500 |
| `performance.issues.db_main_thread.problem-creation` | inferred | 1 |  | 7,052,900 |
| `performance.issues.file_io_on_main_thread.total_spans_duration_threshold` | inferred | 2 |  | 7,052,500 |
| `performance.issues.n_plus_one_db.problem-creation` | inferred | 5 |  | 7,051,800 |
| `performance.issues.query_injection.problem-creation` | inferred | 1 |  | 7,050,700 |
| `performance.issues.consecutive_http.problem-creation` | inferred | 2 |  | 7,050,400 |
| `performance.issues.file_io_main_thread.problem-creation` | inferred | 2 |  | 7,050,100 |
| `performance.issues.render_blocking_assets.problem-creation` | inferred | 1 |  | 7,045,900 |
| `performance.issues.compressed_assets.problem-creation` | inferred | 1 |  | 7,045,600 |
| `performance.issues.render_blocking_assets.size_threshold` | inferred | 1 |  | 7,041,100 |
| `performance.issues.large_http_payload.problem-creation` | inferred | 1 |  | 7,035,600 |
| `performance.issues.db_on_main_thread.total_spans_duration_threshold` | inferred | 2 |  | 7,035,300 |
| `performance.issues.consecutive_http.span_duration_threshold` | inferred | 1 |  | 7,035,000 |
| `performance.issues.n_plus_one_db.duration_threshold` | inferred | 3 |  | 7,034,400 |
| `performance.issues.render_blocking_assets.fcp_maximum_threshold` | inferred | 1 |  | 7,030,800 |
| `performance.issues.web_vitals.count_threshold` | inferred | 2 |  | 7,027,600 |
| `performance.issues.sql_injection.problem-creation` | inferred | 1 |  | 6,988,600 |
| `transaction-events.force-disable-internal-project` | inferred | 1 |  | 5,226,500 |
| `store.s4s-transaction-sample-rate` | inferred | 2 |  | 5,224,600 |
| `replay.recording.ingest-trace-items.rollout` | Float | 1 | DISK | 4,962,800 |
| `store.use-relay-dsn-sample-rate` | inferred | 1 |  | 4,832,200 |
| `groups.enable-post-update-signal` | inferred | 9 |  | 4,328,800 |
| `issues.client_error_sampling.project_allowlist` | Sequence | 9 |  | 3,498,900 |
| `grouping.use_ingest_grouphash_caching` | Bool | 5 |  | 3,359,300 |
| `grouping.config_transition.metrics_sample_rate` | Float | 2 |  | 3,076,700 |
| `nodestore.cache-ttl` | Int | 2 |  | 2,739,100 |
| `store.load-shed-symbolicate-event-projects` | Any | 3 |  | 2,553,000 |
| `grouping.grouphash_metadata.ingestion_writes_enabled` | Bool | 2 |  | 2,390,800 |
| `grouping.experimental_parameterization` | Float | 1 |  | 1,839,800 |
| `sentry-apps.expanded-webhook-categories` | Sequence | 1 |  | 1,674,500 |
| `sentry.similarity.indexing.enabled` | Bool | 2 |  | 1,640,100 |
| `uptime.automatic-hostname-detection` | Bool | 3 |  | 1,632,700 |
| `post-process-forwarder:kafka-headers` | inferred | 1 |  | 1,589,400 |
| `workflow_engine.evaluation_log_sample_rate` | Float | 3 |  | 1,564,100 |
| `workflow_engine.ensure_detector_association` | Bool | 2 |  | 1,560,900 |
| `workflow_engine.group.type_id.disable_issue_stream_detector` | Sequence | 2 |  | 1,552,000 |
| `issues.sdk_crash_detection.native.project_id` | Int | 2 |  | 1,537,700 |
| `kafka.send-project-events-to-random-partitions` | inferred | 2 |  | 1,537,200 |
| `workflow_engine.evaluation_logs_direct_to_sentry` | Bool | 2 |  | 1,534,100 |
| `issues.sdk_crash_detection.cocoa.sample_rate` | inferred | 6 |  | 1,532,100 |
| `issues.sdk_crash_detection.react-native.project_id` | inferred | 5 |  | 1,530,800 |
| `issues.sdk_crash_detection.dart.project_id` | Int | 2 |  | 1,523,600 |
| `issues.sdk_crash_detection.java.sample_rate` | inferred | 3 |  | 1,522,500 |
| `issues.sdk_crash_detection.java.organization_allowlist` | Sequence | 3 |  | 1,520,800 |
| `issues.sdk_crash_detection.react-native.sample_rate` | inferred | 4 |  | 1,519,200 |
| `issues.sdk_crash_detection.dotnet.project_id` | Int | 2 |  | 1,518,600 |
| `issues.sdk_crash_detection.java.project_id` | Int | 3 |  | 1,518,300 |
| `issues.sdk_crash_detection.react-native.organization_allowlist` | Sequence | 4 |  | 1,516,300 |
| `issues.sdk_crash_detection.dotnet.organization_allowlist` | Sequence | 2 |  | 1,514,700 |
| `issues.sdk_crash_detection.dotnet.sample_rate` | inferred | 2 |  | 1,512,600 |
| `issues.sdk_crash_detection.native.sample_rate` | inferred | 2 |  | 1,511,800 |
| `issues.sdk_crash_detection.native.organization_allowlist` | Sequence | 2 |  | 1,504,900 |
| `issues.sdk_crash_detection.cocoa.project_id` | inferred | 6 |  | 1,501,800 |
| `issues.sdk_crash_detection.dart.sample_rate` | inferred | 2 |  | 1,494,200 |
| `issues.sdk_crash_detection.dart.organization_allowlist` | Sequence | 2 |  | 1,493,800 |
| `txnames.bump-lifetime-sample-rate` | inferred | 2 |  | 1,423,300 |
| `store.reprocessing-force-disable` | inferred | 1 |  | 1,323,700 |
| `store.load-shed-process-event-projects` | Any | 3 |  | 1,258,700 |
| `seer.similarity.metrics_sample_rate` | Float | 9 |  | 1,249,100 |
| `store.load-shed-process-event-projects-gradual` | Dict | 2 |  | 1,241,800 |
| `ourlogs.sentry-emit-rollout` | inferred | 1 |  | 1,192,300 |
| `demo-mode.users` | inferred | 12 |  | 858,600 |
| `crons.per_monitor_rate_limit` | Int | 4 | DISK | 428,200 |
| `profiling.killswitch.ingest-profiles` | Sequence | 3 | DISK | 393,900 |
| `replay.storage.backend` | inferred | 3 | DISK | 390,200 |
| `replay.storage.options` | Dict | 3 | DISK | 383,100 |
| `tally-usage-delay-seconds` | Int | 3 |  | 357,900 |
| `project-abuse-quota.attachment-limit` | Int | 3 | DISK | 316,000 |
| `project-abuse-quota.attachment-item-limit` | Int | 3 | DISK | 315,300 |
| `project-abuse-quota.session-limit` | Int | 3 | DISK | 314,800 |
| `getsentry.rate-limit.window` | Int | 2 | DISK | 310,800 |
| `billing.seat-based-seer-launch` | inferred | 3 |  | 283,900 |
| `symbolicator.ignored_sources` | Sequence | 2 |  | 256,000 |
| `sentry-apps.webhook-logging.enabled` | Dict | 2 |  | 244,100 |
| `getsentry.rate-limit.project-errors` | Int | 3 | DISK | 220,300 |
| `spans.buffer.root-timeout` | Int | 3 | DISK | 168,900 |
| `spans.buffer.flusher-cumulative-logger-enabled` | inferred | 3 | DISK | 133,000 |
| `spans.buffer.segment-page-size` | Int | 3 | DISK | 132,700 |
| `spans.buffer.max-flush-segments` | Int | 3 | DISK | 130,800 |
| `sentry-apps.webhook.restricted-webhook-sending` | Sequence | 3 |  | 130,600 |
| `sentry-apps.webhook.circuit-breaker.config` | Dict | 2 |  | 127,000 |
| `apigateway.proxy.circuit-breaker.config` | Dict | 1 |  | 126,400 |
| `crons.organization.disable-check-in` | Sequence | 3 |  | 113,100 |
| `spans.buffer.flusher.flush-lock-ttl` | Int | 4 | DISK | 111,100 |
| `span-metrics-extraction-projects-denylist` | Sequence | 2 |  | 106,100 |
| `span-metrics-extraction-orgs-denylist` | Sequence | 2 |  | 105,900 |
| `span-metrics-extraction-addons-projects-denylist` | Sequence | 2 |  | 101,500 |
| `span-metrics-extraction-enabled` | inferred | 2 |  | 101,500 |
| `spans.buffer.flusher.log-flushed-segments` | inferred | 3 | DISK | 67,500 |
| `data-forwarding.task-rollout-rate` | Float | 6 |  | 44,300 |
| `dynamic-sampling.check_span_feature_flag` | inferred | 6 |  | 41,700 |
| `spans.buffer.timeout` | Int | 3 | DISK | 36,800 |
| `spans.buffer.evalsha-cumulative-logger-enabled` | inferred | 3 | DISK | 36,600 |
| `spans.buffer.flusher.max-unhealthy-seconds` | inferred | 3 | DISK | 36,500 |
| `spans.drop-in-buffer` | Sequence | 3 | DISK | 32,900 |
| `discord.application-id` | inferred | 7 | DISK | 31,700 |
| `uptime.checker-regions-mode-override` | Dict | 6 |  | 12,600 |
| `sms.twilio-account` | inferred | 5 | DISK | 10,700 |
| `dynamic-sampling.measure.spans` | Sequence | 6 |  | 7,800 |
| `demo-mode.enabled` | Bool | 14 |  | 5,200 |
| `dsym.cache-path` | String | 4 | DISK | 3,900 |
| `relocation.enabled` | inferred | 7 |  | 3,000 |
| `spans.buffer.flusher.use-stuck-detector` | inferred | 3 | DISK | 2,300 |
| `chart-rendering.chartcuterie` | inferred | 4 | DISK | 800 |
| `database.encryption.method` | String | 5 | DISK | 700 |
| `github-app.id` | inferred | 11 |  | 700 |
| `demo-mode.orgs` | inferred | 8 |  | 600 |
| `chart-rendering.storage.options` | Dict | 3 | DISK | 500 |
| `replay.viewed-by.project-denylist` | Sequence | 4 | DISK | 300 |
| `chart-rendering.storage.backend` | inferred | 3 | DISK | 200 |
| `api.rate-limit.org-create` | inferred | 5 | DISK | 0 |
| `auth.ip-rate-limit` | inferred | 4 | DISK | 0 |
| `auth.user-rate-limit` | inferred | 4 | DISK | 0 |
| `chart-rendering.enabled` | inferred | 5 | DISK | 0 |
| `github-app.name` | inferred | 7 |  | 0 |
| `github-console-sdk-app.id` | inferred | 6 |  | 0 |
| `notifications.platform-rollout.internal-testing` | Dict | 6 |  | 0 |
| `on_demand_metrics.check_widgets.enable` | inferred | 3 | DISK | 0 |
| `project-abuse-quota.error-limit` | Int | 3 | DISK | 0 |
| `symbolicator.enabled` | inferred | 7 | DISK | 0 |
| `symbolicator.options` | inferred | 4 | DISK | 0 |
| `symbolserver.enabled` | inferred | 6 | DISK | 0 |
| `u2f.facets` | Sequence | 4 | DISK | 0 |
| `user-settings.signed-url-confirmation-emails` | inferred | 3 | DISK | 0 |
| `user-settings.signed-url-confirmation-emails-salt` | String | 3 | DISK | 0 |

## Tier 4: Critical (23 options)

Highest blast radius. Migrate last, after 600+ options proven.

Stats: 8 DISK, 4 complex types

| Option | Type | Usages | DISK | DD Reads |
|--------|------|--------|------|----------|
| `spans.process-segments.schema-validation` | inferred | 3 |  | 68,830,600 |
| `performance.event-tracker.sample-rate.transactions` | inferred | 1 |  | 35,422,700 |
| `flagpole.allowed_features` | Sequence | 4 |  | 31,109,100 |
| `spans.buffer.debug-traces` | Sequence | 3 |  | 30,061,500 |
| `flagpole.log_features` | inferred | 1 |  | 22,872,000 |
| `sentry-metrics.indexer.reconstruct.enable-orjson` | inferred | 1 |  | 18,659,400 |
| `sentry-metrics.indexer.disabled-namespaces` | inferred | 2 |  | 18,623,900 |
| `shared_resources_accounting_enabled` | inferred | 4 |  | 17,280,000 |
| `taskworker.route.overrides` | inferred | 2 |  | 16,685,500 |
| `billing.usagebuffer.batch_incr.projects` | Sequence | 2 |  | 13,482,200 |
| `billing.usagebuffer.batch_incr.rollout` | inferred | 2 |  | 13,451,900 |
| `system.internal-url-prefix` | inferred | 9 | DISK | 1,202,600 |
| `system.debug-files-renewal-age-threshold-days` | inferred | 3 |  | 340,100 |
| `mail.subject-prefix` | inferred | 19 | DISK | 6,000 |
| `mail.enable-replies` | inferred | 11 | DISK | 4,100 |
| `staff.ga-rollout` | Bool | 44 |  | 3,300 |
| `system.support-email` | inferred | 8 | DISK | 3,300 |
| `mail.reply-hostname` | inferred | 10 | DISK | 3,100 |
| `system.event-retention-days` | inferred | 23 | DISK | 2,500 |
| `system.upload-url-prefix` | inferred | 2 | DISK | 1,600 |
| `staff.user-email-allowlist` | Sequence | 1 |  | 100 |
| `superuser.read-write.ga-rollout` | Bool | 20 |  | 100 |
| `system.security-email` | inferred | 8 | DISK | 0 |
