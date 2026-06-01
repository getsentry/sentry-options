# Getsentry Migration Order Proposals

Three proposed orderings for migrating the **556 `NEVER_DELETE` + `FLAG_AUTOMATOR_MODIFIABLE`** options from the old system to sentry-options.

> Generated from `inventory.csv` on the `kjiang/options-inventory` branch.
> 110 of 556 options have `FLAG_PRIORITIZE_DISK` (marked with `*DISK*`) — these need special handling during migration since they currently read from config files before checking DB/cache.

---

## Proposal A: Risk-Based (by usage count)

### Strategy

Migrate options ordered by how many call sites reference them. Fewer call sites = smaller blast radius if the fallback mechanism has a bug.

### Why this order

The usage count is the most direct proxy for "how many things break if this option returns the wrong value." Starting with 0-usage options is essentially free — they're registered but never read by application code. The 1-usage options typically have a single `options.get()` call, so verifying correctness is trivial. By the time you reach the 6+ usage tier, you've battle-tested the system on 519 options.

### Pros

- **Minimizes blast radius at every stage** — 0-usage options can't break anything, 1-usage options can only break one code path
- **Natural confidence ramp** — each tier proves the system works before escalating
- **Front-loads easy wins** — 36 options migrated with near-zero risk, 227 total with ≤1 usage
- **Easy to measure progress** — tiers are objectively defined, no judgment calls

### Cons

- **Crosses namespace/domain boundaries in every tier** — you're touching `backpressure`, `performance`, `seer`, `system` all in the same batch, meaning many teams are affected simultaneously
- **Ignores option importance** — a 1-usage option like `system.event-retention-days` (referenced 23 times in settings files not counted here) is far more critical than a 6-usage feature toggle
- **`FLAG_PRIORITIZE_DISK` scattered everywhere** — you can't defer that complexity to a later tier
- **No clean namespace ownership** — hard to tell one team "your options are done"

### Tiers

#### Tier 1: 0 usage sites (36 options)

Options registered but never directly referenced by `options.get()` in application code. These are either read through dynamic patterns (bounded by the plausibility filter) or are effectively dead. Migrating these is essentially free.

| Option | Type | Flags |
|--------|------|-------|
| `backpressure.high_watermarks.attachments-store` | inferred | `FLAG_AUTOMATOR_MODIFIABLE` |
| `backpressure.high_watermarks.post-process-locks` | inferred | `FLAG_AUTOMATOR_MODIFIABLE` |
| `backpressure.high_watermarks.processing-locks` | inferred | `FLAG_AUTOMATOR_MODIFIABLE` |
| `backpressure.high_watermarks.processing-store` | inferred | `FLAG_AUTOMATOR_MODIFIABLE` |
| `backpressure.high_watermarks.processing-store-transactions` | inferred | `FLAG_AUTOMATOR_MODIFIABLE` |
| `outbox_replication.auth_authenticator.replication_version` | Int | `FLAG_AUTOMATOR_MODIFIABLE` |
| `outbox_replication.auth_user.replication_version` | Int | `FLAG_AUTOMATOR_MODIFIABLE` |
| `outbox_replication.sentry_apikey.replication_version` | Int | `FLAG_AUTOMATOR_MODIFIABLE` |
| `outbox_replication.sentry_apitoken.replication_version` | Int | `FLAG_AUTOMATOR_MODIFIABLE` |
| `outbox_replication.sentry_authidentity.replication_version` | Int | `FLAG_AUTOMATOR_MODIFIABLE` |
| `outbox_replication.sentry_externalactor.replication_version` | Int | `FLAG_AUTOMATOR_MODIFIABLE` |
| `outbox_replication.sentry_organizationavatar.replication_version` | Int | `FLAG_AUTOMATOR_MODIFIABLE` |
| `outbox_replication.sentry_organizationintegration.replication_version` | Int | `FLAG_AUTOMATOR_MODIFIABLE` |
| `outbox_replication.sentry_organizationmember.replication_version` | Int | `FLAG_AUTOMATOR_MODIFIABLE` |
| `outbox_replication.sentry_organizationmember_teams.replication_version` | Int | `FLAG_AUTOMATOR_MODIFIABLE` |
| `outbox_replication.sentry_organizationslugreservation.replication_version` | Int | `FLAG_AUTOMATOR_MODIFIABLE` |
| `outbox_replication.sentry_orgauthtoken.replication_version` | Int | `FLAG_AUTOMATOR_MODIFIABLE` |
| `outbox_replication.sentry_projectkey.replication_version` | Int | `FLAG_AUTOMATOR_MODIFIABLE` |
| `outbox_replication.sentry_sentryappinstallation.replication_version` | Int | `FLAG_AUTOMATOR_MODIFIABLE` |
| `outbox_replication.sentry_sentryappinstallationtoken.replication_version` | Int | `FLAG_AUTOMATOR_MODIFIABLE` |
| `outbox_replication.sentry_team.replication_version` | Int | `FLAG_AUTOMATOR_MODIFIABLE` |
| `outbox_replication.sentry_useremail.replication_version` | Int | `FLAG_AUTOMATOR_MODIFIABLE` |
| `outbox_replication.sentry_userpermission.replication_version` | Int | `FLAG_AUTOMATOR_MODIFIABLE` |
| `outbox_replication.sentry_userrole.replication_version` | Int | `FLAG_AUTOMATOR_MODIFIABLE` |
| `outbox_replication.sentry_userrole_users.replication_version` | Int | `FLAG_AUTOMATOR_MODIFIABLE` |
| `recovery.disallow-new-enrollment` | Bool | *DISK* |
| `sentry-metrics.cardinality-limiter.limits.custom.per-org` | inferred | `FLAG_AUTOMATOR_MODIFIABLE` |
| `sentry-metrics.cardinality-limiter.limits.generic-metrics.per-org` | inferred | `FLAG_AUTOMATOR_MODIFIABLE` |
| `sentry-metrics.cardinality-limiter.limits.profiles.per-org` | inferred | `FLAG_AUTOMATOR_MODIFIABLE` |
| `sentry-metrics.cardinality-limiter.limits.sessions.per-org` | inferred | `FLAG_AUTOMATOR_MODIFIABLE` |
| `sentry-metrics.cardinality-limiter.limits.spans.per-org` | inferred | `FLAG_AUTOMATOR_MODIFIABLE` |
| `sentry-metrics.cardinality-limiter.limits.transactions.per-org` | inferred | `FLAG_AUTOMATOR_MODIFIABLE` |
| `sentry-metrics.writes-limiter.limits.custom.global` | inferred | `FLAG_AUTOMATOR_MODIFIABLE` |
| `sentry-metrics.writes-limiter.limits.custom.per-org` | inferred | `FLAG_AUTOMATOR_MODIFIABLE` |
| `sentry-metrics.writes-limiter.limits.sessions.global` | inferred | `FLAG_AUTOMATOR_MODIFIABLE` |
| `sentry-metrics.writes-limiter.limits.sessions.per-org` | inferred | `FLAG_AUTOMATOR_MODIFIABLE` |

#### Tier 2: 1 usage site (191 options)

Single call-site options. One `options.get()` in one file. Easy to verify, easy to rollback.

| Option | Type | DISK |
|--------|------|------|
| `apigateway.proxy.circuit-breaker.config` | Dict | |
| `apigateway.proxy.circuit-breaker.enabled` | Bool | |
| `apigateway.proxy.circuit-breaker.enforce` | Bool | |
| `apigateway.proxy.timeout` | Int | |
| `aws-lambda.host-region` | inferred | |
| `aws-lambda.node.layer-name` | inferred | |
| `aws-lambda.node.layer-version` | inferred | |
| `aws-lambda.python.layer-name` | inferred | |
| `aws-lambda.python.layer-version` | inferred | |
| `aws-lambda.thread-count` | inferred | |
| `backpressure.monitoring.interval` | inferred | |
| `cleanup.abort_execution` | Bool | |
| `consumer.dump_stacktrace_on_shutdown` | Sequence | |
| `consumer.join.profiling.rate` | Float | |
| `consumer.shared_memory_spawn_process` | Bool | |
| `consumer.verbose_multiprocessing_logs` | Sequence | |
| `data-forwarding.project-cache-ttl` | Int | |
| `deletions.group-hashes-batch-size` | Int | |
| `demo-org-ids` | inferred | |
| `discord.debug-channel` | inferred | |
| `discord.debug-server` | inferred | |
| `dynamic-sampling.config.killswitch` | inferred | |
| `dynamic-sampling:sliding_window.size` | inferred | |
| `eventstore.adjacent_event_ids_use_snql` | Bool | |
| `eventstream:kafka-headers` | inferred | |
| `explorer.service_map.max_segments` | Int | *DISK* |
| `explorer.service_map.parent_span_batch_size` | Int | *DISK* |
| `features.error.capture_rate` | inferred | |
| `getsentry.rate-limit.project-transactions` | Int | *DISK* |
| `github-console-sdk-app.client-id` | inferred | |
| `github-login.api-domain` | inferred | *DISK* |
| `github-login.base-domain` | inferred | *DISK* |
| `github-login.organization` | inferred | *DISK* |
| `github-login.require-verified-email` | Bool | *DISK* |
| `grouping.experimental_parameterization` | Float | |
| `hybrid_cloud.authentication.use_api_key_replica` | Bool | |
| `hybrid_cloud.disable_tombstone_cleanup` | inferred | |
| `hybridcloud.integrationproxy.retries` | inferred | |
| `hybridcloud.regionsiloclient.retries` | inferred | |
| `hybridcloud.rpc.use_pooling.rate` | Float | |
| `hybridcloud.webhookpayload.skip_on_failure_providers` | Sequence | |
| `integrations.backfill_github_external_actor.gh_api_fetch_interval_s` | Float | |
| `integrations.slo.integration-id-tag-enabled` | Bool | |
| `issue-detection.llm-detection.enabled` | Bool | |
| `issue-detection.web-vitals-detection.projects-allowlist` | Sequence | |
| `issues.group_events.batch_nodestore_enabled` | Bool | |
| `issues.severity.seer-circuit-breaker-passthrough-limit` | Dict | |
| `issues.severity.seer-global-rate-limit` | Any | |
| `issues.severity.seer-project-rate-limit` | Any | |
| `notifications.platform-rollout.early-adopter` | Dict | |
| `notifications.platform-rollout.general-access` | Dict | |
| `notifications.platform.killswitch.sources` | Sequence | |
| `on_demand.extended_alert_spec_orgs` | inferred | |
| `on_demand.extended_max_alert_specs` | inferred | |
| `on_demand.max_widget_cardinality.killswitch` | inferred | |
| `on_demand.update_on_demand_modified` | inferred | |
| `options_automator_slack_webhook_enabled` | inferred | |
| `organization-abuse-quota.metric-bucket-limit` | Int | *DISK* |
| `organization.default-owner-id-cache-ttl` | Int | |
| `ourlogs.sentry-emit-rollout` | inferred | |
| `outbox_replication.sentry_authprovider.replication_version` | Int | |
| `outbox_replication.sentry_organization.replication_version` | Int | |
| `performance.event-tracker.sample-rate.transactions` | inferred | |
| `performance.extrapolation.confidence.z-score` | Float | |
| `performance.issues.compressed_assets.problem-creation` | inferred | |
| `performance.issues.consecutive_db.problem-creation` | inferred | |
| `performance.issues.consecutive_http.consecutive_count_threshold` | inferred | |
| `performance.issues.consecutive_http.max_duration_between_spans` | inferred | |
| `performance.issues.consecutive_http.span_duration_threshold` | inferred | |
| `performance.issues.db_main_thread.problem-creation` | inferred | |
| `performance.issues.http_overhead.http_request_delay_threshold` | inferred | |
| `performance.issues.http_overhead.problem-creation` | inferred | |
| `performance.issues.large_http_payload.filtered_paths` | inferred | |
| `performance.issues.large_http_payload.problem-creation` | inferred | |
| `performance.issues.m_n_plus_one_db.problem-creation` | inferred | |
| `performance.issues.query_injection.problem-creation` | inferred | |
| `performance.issues.render_blocking_assets.fcp_maximum_threshold` | inferred | |
| `performance.issues.render_blocking_assets.fcp_minimum_threshold` | inferred | |
| `performance.issues.render_blocking_assets.problem-creation` | inferred | |
| `performance.issues.render_blocking_assets.size_threshold` | inferred | |
| `performance.issues.sql_injection.problem-creation` | inferred | |
| `performance.issues.sql_injection.query_value_length_threshold` | inferred | |
| `performance.spans-tags-key.max` | Int | |
| `performance.spans-tags-values.max` | Int | |
| `performance.trace.span_with_errors_ok_status.sample_rate` | Float | |
| `performance.traces.check_span_extraction_date` | Bool | |
| `performance.traces.query_timestamp_projects` | Bool | |
| `performance.traces.span_query_timebuffer_hours` | Float | |
| `performance.traces.trace-explorer-skip-recent-seconds` | Int | |
| `post-process-forwarder:kafka-headers` | inferred | |
| `processing.severity-backlog-test.error` | inferred | |
| `processing.severity-backlog-test.timeout` | inferred | |
| `profiling.continuous-profiling.chunks-query.size` | Int | |
| `profiling.flamegraph.query.initial_chunk_delta.hours` | Int | |
| `profiling.flamegraph.query.max_delta.hours` | Int | |
| `profiling.flamegraph.query.multiplier` | Int | |
| `project-abuse-quota.span-limit` | Int | *DISK* |
| `project-abuse-quota.transaction-limit` | Int | *DISK* |
| `relay.drop-transaction-attachments` | Bool | |
| `relay.endpoint-fetch-config.enabled` | Bool | |
| `relay.kafka.span-v2.sample-rate` | Float | |
| `relocation.daily-limit.medium` | inferred | |
| `relocation.outbox-orgslug.killswitch` | Sequence | |
| `relocation.selectable-regions` | inferred | |
| `replay.consumer.msgspec_recording_parser` | Bool | |
| `replay.endpoints.project_replay_summary.trace_sample_rate_get` | inferred | |
| `replay.endpoints.project_replay_summary.trace_sample_rate_post` | inferred | |
| `replay.recording.ingest-trace-items.rollout` | Float | *DISK* |
| `sdk-deprecation.profile-chunk.cocoa.hard` | inferred | |
| `sdk-deprecation.profile-chunk.cocoa.reject` | inferred | |
| `sdk-deprecation.profile-chunk.python.hard` | inferred | |
| `sdk-deprecation.profile.cocoa.reject` | inferred | |
| `sdk_http2_experiment.enabled` | Bool | |
| `seer.api.use-shared-secret` | inferred | |
| `seer.explorer-index.rollout` | Float | |
| `seer.explorer.context-engine-rollout` | Float | |
| `seer.explorer_index.enable` | Bool | |
| `seer.similarity-embeddings-delete-by-hash-killswitch.enabled` | Bool | |
| `seer.similarity.global-rate-limit` | Dict | |
| `seer.similarity.ingest.store_hybrid_fingerprint_non_matches` | Bool | |
| `seer.similarity.per-project-rate-limit` | Dict | |
| `seer.supergroups_backfill_lightweight.inter_batch_delay_s` | Int | |
| `seer.supergroups_backfill_lightweight.max_failures_per_batch` | Int | |
| `sentry-apps.expanded-webhook-categories` | Sequence | |
| `sentry-apps.webhook.circuit-breaker.dry-run` | Bool | |
| `sentry-metrics.indexer.disable-memcache-replenish-rollout` | inferred | |
| `sentry-metrics.indexer.generic-metrics.schema-validation-rules` | inferred | |
| `sentry-metrics.indexer.reconstruct.enable-orjson` | inferred | |
| `sentry-metrics.indexer.release-health.schema-validation-rules` | inferred | |
| `sentry-metrics.synchronized-rebalance-delay` | inferred | |
| `sentry-metrics.writes-limiter.limits.generic-metrics.global` | inferred | |
| `sentry-metrics.writes-limiter.limits.generic-metrics.per-org` | inferred | |
| `sentry-metrics.writes-limiter.limits.performance.global` | inferred | |
| `sentry-metrics.writes-limiter.limits.performance.per-org` | inferred | |
| `sentry-metrics.writes-limiter.limits.releasehealth.global` | inferred | |
| `sentry-metrics.writes-limiter.limits.releasehealth.per-org` | inferred | |
| `sentry-metrics.writes-limiter.limits.spans.global` | inferred | |
| `sentry-metrics.writes-limiter.limits.spans.per-org` | inferred | |
| `sentry-metrics.writes-limiter.limits.transactions.global` | inferred | |
| `sentry-metrics.writes-limiter.limits.transactions.per-org` | inferred | |
| `sentry.demo_mode.sync_debug_artifacts.enable` | Bool | *DISK* |
| `sentry.demo_mode.sync_debug_artifacts.source_org_id` | Int | *DISK* |
| `sentry.save-event-attachments.project-per-5-minute-limit` | Int | |
| `sentry.save-event-attachments.project-per-sec-limit` | Int | |
| `sentry.scm.stream.rollout` | Float | *DISK* |
| `sentry.search.events.project.check_event` | Float | |
| `sentry.send_onboarding_task_metrics` | Bool | |
| `sentry:skip-record-onboarding-tasks-if-complete` | Bool | |
| `slack.debug-channel` | inferred | |
| `slack.debug-workspace` | inferred | |
| `slack.log-unfurl-payload` | inferred | |
| `snuba.search.chunk-growth-rate` | inferred | |
| `snuba.search.max-chunk-size` | inferred | |
| `snuba.search.max-total-chunk-time-seconds` | inferred | |
| `snuba.search.min-pre-snuba-candidates` | inferred | |
| `snuba.search.pre-snuba-candidates-optimizer` | Bool | |
| `snuba.search.recommended.event-volume-weight` | inferred | |
| `snuba.search.recommended.recency-weight` | inferred | |
| `snuba.search.recommended.severity-weight` | inferred | |
| `snuba.search.recommended.spike-weight` | inferred | |
| `snuba.search.recommended.user-impact-weight` | inferred | |
| `snuba.tagstore.cache-tagkeys-rate` | inferred | *DISK* |
| `spans.buffer.evalsha-latency-threshold` | Int | |
| `spans.process-spans.profiling.rate` | Float | *DISK* |
| `staff.user-email-allowlist` | Sequence | |
| `statistical_detectors.query.batch_size` | Int | *DISK* |
| `statistical_detectors.query.functions.timeseries_days` | Int | *DISK* |
| `statistical_detectors.query.transactions.timeseries_days` | Int | *DISK* |
| `statistical_detectors.throughput.threshold.functions` | Int | |
| `statistical_detectors.throughput.threshold.transactions` | Int | |
| `store.allow-s4s-ddm-sample-rate` | inferred | |
| `store.reprocessing-force-disable` | inferred | |
| `store.use-relay-dsn-sample-rate` | inferred | |
| `subscriptions-query.sample-rate` | inferred | |
| `symbolicate.symx-logging-rate` | inferred | |
| `symbolicate.symx-os-description-list` | inferred | |
| `symbolicator.sourcemaps-bundle-index-refresh-sample-rate` | inferred | |
| `symbolserver.options` | inferred | *DISK* |
| `taskworker.producer.max_futures` | Int | |
| `tempest.poll-limit` | inferred | |
| `totp.disallow-new-enrollment` | Bool | *DISK* |
| `transaction-events.force-disable-internal-project` | inferred | |
| `u2f.app-id` | inferred | *DISK* |
| `u2f.disallow-new-enrollment` | Bool | *DISK* |
| `uptime.use-detectors-by-data-source-cache` | Bool | |
| `vercel.integration-slug` | inferred | |
| `visibility.tag-key-sample-size` | inferred | |
| `vsts.social-auth-migration` | Bool | |
| `workflow_engine.group.type_id.open_periods_type_denylist` | Sequence | |
| `workflow_engine.num_cohorts` | Int | |
| `workflow_engine.schedule.min_cohort_scheduling_age_seconds` | Int | |

#### Tier 3: 2 usage sites (189 options)

Typically an `options.get()` call + one test. Full list omitted for brevity — see the raw data in `inventory.csv` filtered by `usages=2`.

Notable inclusions: `api-token-async-flush`, `api.deprecation.*`, `autopilot.*`, `crons.system_incidents.*`, most `performance.issues.*` thresholds, `relay.*` rollout rates, `seer.*` rate limits, `spans.buffer.*` configs (*DISK*), `workflow_engine.*`.

#### Tier 4: 3-5 usage sites (103 options)

Multi-file references — the option is read in several modules, often with tests and settings files.

Notable inclusions: `auth.ip-rate-limit` (*DISK*), `chart-rendering.*` (*DISK*), `crons.per_monitor_rate_limit` (*DISK*), `project-abuse-quota.*` (*DISK*), `replay.storage.*` (*DISK*), `spans.buffer.*` (*DISK*), `sentry-apps.*`, `seer.similarity.*`.

#### Tier 5: 6+ usage sites (37 options)

The most heavily referenced options. Maximum caution required.

| Option | Type | Usages | DISK |
|--------|------|--------|------|
| `demo-mode.enabled` | Bool | 14 | |
| `demo-mode.orgs` | inferred | 8 | |
| `demo-mode.users` | inferred | 12 | |
| `discord.application-id` | inferred | 7 | *DISK* |
| `discord.public-key` | inferred | 6 | *DISK* |
| `dynamic-sampling.check_span_feature_flag` | inferred | 6 | |
| `dynamic-sampling.measure.spans` | Sequence | 6 | |
| `github-app.client-id` | inferred | 12 | *DISK* |
| `github-app.id` | inferred | 11 | |
| `github-app.name` | inferred | 7 | |
| `github-console-sdk-app.id` | inferred | 6 | |
| `github-login.client-id` | inferred | 7 | *DISK* |
| `groups.enable-post-update-signal` | inferred | 9 | |
| `issues.client_error_sampling.project_allowlist` | Sequence | 9 | |
| `issues.sdk_crash_detection.cocoa.project_id` | inferred | 6 | |
| `issues.sdk_crash_detection.cocoa.sample_rate` | inferred | 6 | |
| `issues.search.use-tag-aware-condition-resolver` | Bool | 10 | |
| `mail.enable-replies` | inferred | 11 | *DISK* |
| `mail.mailgun-api-key` | inferred | 10 | *DISK* |
| `mail.reply-hostname` | inferred | 10 | *DISK* |
| `mail.subject-prefix` | inferred | 19 | *DISK* |
| `msteams.client-id` | inferred | 7 | *DISK* |
| `notifications.platform-rollout.internal-testing` | Dict | 6 | |
| `performance.issues.all.problem-detection` | inferred | 6 | |
| `relocation.enabled` | inferred | 7 | |
| `seer.similarity.metrics_sample_rate` | Float | 9 | |
| `slack.client-id` | inferred | 6 | *DISK* |
| `staff.ga-rollout` | Bool | 43 | |
| `superuser.read-write.ga-rollout` | Bool | 20 | |
| `symbolicator.enabled` | inferred | 7 | *DISK* |
| `symbolserver.enabled` | inferred | 6 | *DISK* |
| `system.event-retention-days` | inferred | 23 | *DISK* |
| `system.internal-url-prefix` | inferred | 9 | *DISK* |
| `system.security-email` | inferred | 8 | *DISK* |
| `system.support-email` | inferred | 8 | *DISK* |
| `uptime.checker-regions-mode-override` | Dict | 6 | |
| `vercel.client-id` | inferred | 6 | *DISK* |

---

## Proposal B: Namespace-Based (by domain)

### Strategy

Migrate entire namespaces at once, ordered by domain risk. Start with internal infrastructure plumbing that engineers rarely interact with directly, then product feature knobs, then third-party integration configs, and finally system/core options.

### Why this order

Internal infrastructure options (`outbox_replication`, `backpressure`, `sentry-metrics`) are managed by a small number of engineers and have well-understood behavior. Product feature options are tuned frequently but are typically rollout gates and thresholds with known-safe defaults. Integration configs (`github-app`, `slack`, `discord`) are more sensitive because they affect external service connectivity. System/core options (`system.*`, `mail.*`, `auth.*`) are the most critical — a wrong value can take down authentication, email, or rate limiting.

### Pros

- **Clean ownership boundaries** — you can notify one team at a time ("we're migrating all `performance.*` options next week")
- **Whole-namespace migration** means the schema file is complete for that domain, no half-migrated namespaces confusing engineers
- **Easier automator coordination** — one namespace = one `values.yaml` in sentry-options-automator
- **`FLAG_PRIORITIZE_DISK` options are clustered** — `spans` has 21, `system` has 5, `mail` has 5 — you handle that complexity per-batch instead of encountering it randomly
- **Matches how people think about the system** — "are the performance options migrated yet?" is a natural question

### Cons

- **Tier 2 is massive** (314 options) — impractical to migrate all product feature options at once; you'd need sub-tiers
- **Risk isn't uniform within a tier** — `performance` knobs (low-risk thresholds) sit alongside `spans.buffer.*` (high-risk infra tuning) in the same "product" tier
- **Type complexity scattered** — you hit `Dict`, `Sequence`, and `Any` types in every tier, so you can't incrementally validate your schema tooling
- **Cross-namespace dependencies not captured** — `seer.*` options are used alongside `issues.*`, but they'd be in the same tier anyway

### Tiers

#### Tier 1: Internal Infrastructure (85 options)

Plumbing that end-users and most engineers never touch directly. Low average usage (1.0), many zero-usage options, primarily `Int` and `inferred` types.

**Namespaces:** `backpressure` (10), `consumer` (4), `data-forwarding` (1), `deletions` (2), `eventstore` (1), `eventstream` (3), `kafka` (1), `nodestore` (2), `objectstore` (1), `outbox_replication` (22), `post-process-forwarder` (1), `processing` (2), `sentry-metrics` (31), `subscriptions-query` (1), `taskworker` (2), `transaction-events` (1)

| Option | Type | Usages | DISK |
|--------|------|--------|------|
| `backpressure.checking.enabled` | inferred | 3 | |
| `backpressure.checking.interval` | inferred | 2 | |
| `backpressure.high_watermarks.attachments-store` | inferred | 0 | |
| `backpressure.high_watermarks.post-process-locks` | inferred | 0 | |
| `backpressure.high_watermarks.processing-locks` | inferred | 0 | |
| `backpressure.high_watermarks.processing-store` | inferred | 0 | |
| `backpressure.high_watermarks.processing-store-transactions` | inferred | 0 | |
| `backpressure.monitoring.enabled` | inferred | 4 | |
| `backpressure.monitoring.interval` | inferred | 1 | |
| `backpressure.status_ttl` | inferred | 3 | |
| `consumer.dump_stacktrace_on_shutdown` | Sequence | 1 | |
| `consumer.join.profiling.rate` | Float | 1 | |
| `consumer.shared_memory_spawn_process` | Bool | 1 | |
| `consumer.verbose_multiprocessing_logs` | Sequence | 1 | |
| `data-forwarding.project-cache-ttl` | Int | 1 | |
| `deletions.group-hash-metadata.batch-size` | Int | 2 | |
| `deletions.group-hashes-batch-size` | Int | 1 | |
| `eventstore.adjacent_event_ids_use_snql` | Bool | 1 | |
| `eventstream.eap.deletion-enabled` | Bool | 2 | |
| `eventstream.eap_forwarding_rate` | inferred | 3 | |
| `eventstream:kafka-headers` | inferred | 1 | |
| `kafka.send-project-events-to-random-partitions` | inferred | 2 | |
| `nodestore.cache-ttl` | Int | 2 | |
| `nodestore.set-subkeys.enable-set-cache-item` | inferred | 2 | |
| `objectstore.enable_for.attachments` | inferred | 4 | |
| `outbox_replication.auth_authenticator.replication_version` | Int | 0 | |
| `outbox_replication.auth_user.replication_version` | Int | 0 | |
| `outbox_replication.sentry_apikey.replication_version` | Int | 0 | |
| `outbox_replication.sentry_apitoken.replication_version` | Int | 0 | |
| `outbox_replication.sentry_authidentity.replication_version` | Int | 0 | |
| `outbox_replication.sentry_authprovider.replication_version` | Int | 1 | |
| `outbox_replication.sentry_externalactor.replication_version` | Int | 0 | |
| `outbox_replication.sentry_organization.replication_version` | Int | 1 | |
| `outbox_replication.sentry_organizationavatar.replication_version` | Int | 0 | |
| `outbox_replication.sentry_organizationintegration.replication_version` | Int | 0 | |
| `outbox_replication.sentry_organizationmember.replication_version` | Int | 0 | |
| `outbox_replication.sentry_organizationmember_teams.replication_version` | Int | 0 | |
| `outbox_replication.sentry_organizationslugreservation.replication_version` | Int | 0 | |
| `outbox_replication.sentry_orgauthtoken.replication_version` | Int | 0 | |
| `outbox_replication.sentry_projectkey.replication_version` | Int | 0 | |
| `outbox_replication.sentry_sentryappinstallation.replication_version` | Int | 0 | |
| `outbox_replication.sentry_sentryappinstallationtoken.replication_version` | Int | 0 | |
| `outbox_replication.sentry_team.replication_version` | Int | 0 | |
| `outbox_replication.sentry_useremail.replication_version` | Int | 0 | |
| `outbox_replication.sentry_userpermission.replication_version` | Int | 0 | |
| `outbox_replication.sentry_userrole.replication_version` | Int | 0 | |
| `outbox_replication.sentry_userrole_users.replication_version` | Int | 0 | |
| `post-process-forwarder:kafka-headers` | inferred | 1 | |
| `processing.severity-backlog-test.error` | inferred | 1 | |
| `processing.severity-backlog-test.timeout` | inferred | 1 | |
| `sentry-metrics.10s-granularity` | inferred | 2 | |
| `sentry-metrics.cardinality-limiter.limits.custom.per-org` | inferred | 0 | |
| `sentry-metrics.cardinality-limiter.limits.generic-metrics.per-org` | inferred | 0 | |
| `sentry-metrics.cardinality-limiter.limits.profiles.per-org` | inferred | 0 | |
| `sentry-metrics.cardinality-limiter.limits.sessions.per-org` | inferred | 0 | |
| `sentry-metrics.cardinality-limiter.limits.spans.per-org` | inferred | 0 | |
| `sentry-metrics.cardinality-limiter.limits.transactions.per-org` | inferred | 0 | |
| `sentry-metrics.drop-percentiles.per-use-case` | inferred | 3 | |
| `sentry-metrics.indexer.disable-memcache-replenish-rollout` | inferred | 1 | |
| `sentry-metrics.indexer.disabled-namespaces` | inferred | 2 | |
| `sentry-metrics.indexer.generic-metrics.schema-validation-rules` | inferred | 1 | |
| `sentry-metrics.indexer.read-new-cache-namespace` | inferred | 3 | |
| `sentry-metrics.indexer.reconstruct.enable-orjson` | inferred | 1 | |
| `sentry-metrics.indexer.release-health.schema-validation-rules` | inferred | 1 | |
| `sentry-metrics.indexer.write-new-cache-namespace` | inferred | 3 | |
| `sentry-metrics.releasehealth.abnormal-mechanism-extraction-rate` | inferred | 3 | |
| `sentry-metrics.synchronized-rebalance-delay` | inferred | 1 | |
| `sentry-metrics.writes-limiter.limits.custom.global` | inferred | 0 | |
| `sentry-metrics.writes-limiter.limits.custom.per-org` | inferred | 0 | |
| `sentry-metrics.writes-limiter.limits.generic-metrics.global` | inferred | 1 | |
| `sentry-metrics.writes-limiter.limits.generic-metrics.per-org` | inferred | 1 | |
| `sentry-metrics.writes-limiter.limits.performance.global` | inferred | 1 | |
| `sentry-metrics.writes-limiter.limits.performance.per-org` | inferred | 1 | |
| `sentry-metrics.writes-limiter.limits.releasehealth.global` | inferred | 1 | |
| `sentry-metrics.writes-limiter.limits.releasehealth.per-org` | inferred | 1 | |
| `sentry-metrics.writes-limiter.limits.sessions.global` | inferred | 0 | |
| `sentry-metrics.writes-limiter.limits.sessions.per-org` | inferred | 0 | |
| `sentry-metrics.writes-limiter.limits.spans.global` | inferred | 1 | |
| `sentry-metrics.writes-limiter.limits.spans.per-org` | inferred | 1 | |
| `sentry-metrics.writes-limiter.limits.transactions.global` | inferred | 1 | |
| `sentry-metrics.writes-limiter.limits.transactions.per-org` | inferred | 1 | |
| `subscriptions-query.sample-rate` | inferred | 1 | |
| `taskworker.producer.max_futures` | Int | 1 | |
| `taskworker.route.overrides` | inferred | 2 | |
| `transaction-events.force-disable-internal-project` | inferred | 1 | |

#### Tier 2: Product Features (314 options)

The bulk of the migration. These are the feature toggles, rollout rates, thresholds, and tuning knobs that product teams use day-to-day. This tier is too large for a single batch — it would need to be broken into sub-tiers (e.g., by namespace size or by team).

**Namespaces:** `performance` (50), `seer` (29), `issues` (27), `spans` (26), `snuba` (15), `store` (11), `profiling` (10), `replay` (10), `sentry-apps` (10), `workflow_engine` (10), `on_demand` (10), `dynamic-sampling` (10), `crons` (8), `uptime` (7), `statistical_detectors` (7), `relocation` (8), `tempest` (6), `sdk-deprecation` (6), `on_demand_metrics` (6), `grouping` (5), `notifications` (5), `explorer` (4), `feedback` (3), `issue-detection` (4), `release-health` (3), `demo-mode` (4), `autopilot` (3), `metric_alerts` (2), `explore` (2), `embeddings-grouping` (1), `delayed_processing` (1), `delayed_workflow` (1), `devtoolbar` (1), `insights` (1), `ourlogs` (1), `post_process` (1), `reprocessing2` (1), `secret-scanning` (1), `similarity` (1), `txnames` (1), `dashboards` (1)

Full option list omitted — see `inventory.csv` for the complete set.

#### Tier 3: Integrations (49 options)

Third-party service configuration. Many of these are `*DISK*` options containing client IDs, API keys, and service URLs that are set per-environment. These are sensitive because a wrong value breaks external service connectivity.

**Namespaces:** `aws-lambda` (9), `chart-rendering` (4), `discord` (4), `github-app` (5), `github-console-sdk-app` (2), `github-enterprise-app` (1), `github-login` (6), `github` (1), `msteams` (1), `pagerduty` (1), `slack` (4), `slack-staging` (1), `sms` (3), `vercel` (2), `vsts` (3), `vsts-limited` (1), `vsts_new` (1)

| Option | Type | Usages | DISK |
|--------|------|--------|------|
| `aws-lambda.access-key-id` | inferred | 3 | *DISK* |
| `aws-lambda.account-number` | inferred | 4 | |
| `aws-lambda.cloudformation-url` | inferred | 4 | |
| `aws-lambda.host-region` | inferred | 1 | |
| `aws-lambda.node.layer-name` | inferred | 1 | |
| `aws-lambda.node.layer-version` | inferred | 1 | |
| `aws-lambda.python.layer-name` | inferred | 1 | |
| `aws-lambda.python.layer-version` | inferred | 1 | |
| `aws-lambda.thread-count` | inferred | 1 | |
| `chart-rendering.chartcuterie` | inferred | 4 | *DISK* |
| `chart-rendering.enabled` | inferred | 5 | *DISK* |
| `chart-rendering.storage.backend` | inferred | 3 | *DISK* |
| `chart-rendering.storage.options` | Dict | 3 | *DISK* |
| `discord.application-id` | inferred | 7 | *DISK* |
| `discord.debug-channel` | inferred | 1 | |
| `discord.debug-server` | inferred | 1 | |
| `discord.public-key` | inferred | 6 | *DISK* |
| `github-app.client-id` | inferred | 12 | *DISK* |
| `github-app.fetch-commits.max-compare-commits` | Int | 2 | |
| `github-app.id` | inferred | 11 | |
| `github-app.name` | inferred | 7 | |
| `github-app.rate-limit-sensitive-orgs` | Sequence | 2 | |
| `github-console-sdk-app.client-id` | inferred | 1 | |
| `github-console-sdk-app.id` | inferred | 6 | |
| `github-enterprise-app.allowed-hosts-legacy-webhooks` | Sequence | 2 | |
| `github-login.api-domain` | inferred | 1 | *DISK* |
| `github-login.base-domain` | inferred | 1 | *DISK* |
| `github-login.client-id` | inferred | 7 | *DISK* |
| `github-login.extended-permissions` | Sequence | 2 | *DISK* |
| `github-login.organization` | inferred | 1 | *DISK* |
| `github-login.require-verified-email` | Bool | 1 | *DISK* |
| `github.webhook.mailbox-bucketing.enabled` | inferred | 2 | |
| `msteams.client-id` | inferred | 7 | *DISK* |
| `pagerduty.app-id` | inferred | 5 | |
| `slack-staging.client-id` | inferred | 3 | *DISK* |
| `slack.client-id` | inferred | 6 | *DISK* |
| `slack.debug-channel` | inferred | 1 | |
| `slack.debug-workspace` | inferred | 1 | |
| `slack.log-unfurl-payload` | inferred | 1 | |
| `sms.disallow-new-enrollment` | Bool | 3 | |
| `sms.twilio-account` | inferred | 5 | *DISK* |
| `sms.twilio-number` | inferred | 2 | *DISK* |
| `vercel.client-id` | inferred | 6 | *DISK* |
| `vercel.integration-slug` | inferred | 1 | |
| `vsts-limited.client-id` | inferred | 4 | *DISK* |
| `vsts.client-id` | inferred | 4 | *DISK* |
| `vsts.consent-prompt` | inferred | 3 | |
| `vsts.social-auth-migration` | Bool | 1 | |
| `vsts_new.client-id` | inferred | 2 | *DISK* |

#### Tier 4: System/Core (108 options)

The most critical options in the monolith. Includes authentication, rate limiting, email, abuse quotas, relay, hybrid cloud, and system-level configuration. These should be migrated last with the most caution. Many are `*DISK*` and have high usage counts.

**Namespaces:** `api` (5), `apigateway` (4), `auth` (2), `billing` (0 — not AUTOMATOR_MODIFIABLE), `chunk-upload` (1), `cleanup` (1), `database` (1), `dsym` (1), `features` (1), `flags` (2), `getsentry` (3), `groups` (1), `hybrid_cloud` (7), `hybridcloud` (9), `integrations` (2), `mail` (5), `options_automator_slack_webhook_enabled` (1), `organization` (1), `organization-abuse-quota` (1), `project-abuse-quota` (7), `provision_organization` (2), `recovery` (1), `relay` (13), `releasefile` (1), `sdk_http2_experiment` (1), `sentry` (8), `staff` (2), `superuser` (1), `symbolicate` (2), `symbolicator` (4), `symbolserver` (2), `system` (6), `totp` (1), `u2f` (3), `user-settings` (2), `visibility` (2)

Full option list omitted for brevity — 108 options across these namespaces. Key high-risk options include:

- `system.event-retention-days` (23 usages, *DISK*)
- `staff.ga-rollout` (43 usages)
- `superuser.read-write.ga-rollout` (20 usages)
- `mail.subject-prefix` (19 usages, *DISK*)
- `system.internal-url-prefix` (9 usages, *DISK*)
- `auth.ip-rate-limit` / `auth.user-rate-limit` (4 usages each, *DISK*)
- All `project-abuse-quota.*` (7 options, all *DISK*)

---

## Proposal C: Type-Complexity-Based

### Strategy

Migrate by schema type complexity. Start with `Bool` (simplest — two possible values), then numeric types, then inferred types (split by default complexity), and finally collection types (`Sequence`, `Dict`, `Any`).

### Why this order

Each tier introduces one new level of schema definition complexity. `Bool` options need only `"type": "boolean"` in the schema. `Int`/`Float`/`String` add numeric/string validation. `inferred` types require you to determine the actual type from the default value and code. `Sequence`/`Dict`/`Any` require `items` or `properties` definitions and are the hardest to get right in the schema.

This order validates your schema tooling incrementally — if Bool options work, you know basic schema loading is correct before moving to types that need more complex schema definitions.

### Pros

- **Incremental schema complexity** — each tier adds one new capability
- **`Bool` options are trivially verifiable** — only `true` or `false`, impossible to have subtle type mismatches
- **Defers the hardest work** — `Dict` and `Any` options need careful schema design (especially things like `apigateway.proxy.circuit-breaker.config` with nested structures)
- **`inferred` split is useful** — 196 options have simple defaults (`False`, `0`, `[]`) where the type is obvious, vs. 77 with complex defaults that need investigation

### Cons

- **Completely ignores business risk** — `staff.ga-rollout` (43 usages, controls staff access across the entire app) is in Tier 1 because it's a Bool
- **Crosses every namespace in every tier** — impossible to coordinate with specific teams
- **`FLAG_PRIORITIZE_DISK` options appear everywhere** — no way to handle them as a batch
- **The inferred type split is somewhat arbitrary** — some "simple" defaults like `{}` could map to either an empty Dict or an object with specific properties
- **8 `Any`-typed options need type investigation** regardless — they're in Tier 5 but their actual type needs to be determined from the code

### Tiers

#### Tier 1: Bool (68 options)

Binary toggles, killswitches, and feature enables. Schema definition is trivial: `"type": "boolean"`.

| Option | Default | Usages | DISK |
|--------|---------|--------|------|
| `api-token-async-flush` | False | 2 | |
| `apigateway.proxy.circuit-breaker.enabled` | False | 1 | |
| `apigateway.proxy.circuit-breaker.enforce` | False | 1 | |
| `cleanup.abort_execution` | False | 1 | |
| `consumer.shared_memory_spawn_process` | False | 1 | |
| `delayed_workflow.rollout` | False | 3 | |
| `demo-mode.enabled` | False | 14 | |
| `devtoolbar.analytics.enabled` | False | 2 | *DISK* |
| `eventstore.adjacent_event_ids_use_snql` | False | 1 | |
| `eventstream.eap.deletion-enabled` | True | 2 | |
| `explorer.context_engine_indexing.enable` | False | 5 | |
| `feedback.filter_garbage_messages` | False | 2 | *DISK* |
| `flags:options-audit-log-is-enabled` | True | 2 | *DISK* |
| `github-login.require-verified-email` | False | 1 | *DISK* |
| `grouping.grouphash_metadata.ingestion_writes_enabled` | True | 2 | |
| `grouping.use_ingest_grouphash_caching` | True | 5 | |
| `hybrid_cloud.authentication.use_api_key_replica` | False | 1 | |
| `integrations.slo.integration-id-tag-enabled` | False | 1 | |
| `issue-detection.llm-detection.enabled` | False | 1 | |
| `issue-detection.web-vitals-detection.enabled` | False | 2 | |
| `issues.group_events.batch_nodestore_enabled` | True | 1 | |
| `issues.occurrence-consumer.rate-limit.enabled` | False | 2 | |
| `issues.search.use-tag-aware-condition-resolver` | False | 10 | |
| `performance.traces.check_span_extraction_date` | False | 1 | |
| `performance.traces.query_timestamp_projects` | False | 1 | |
| `profiling.profile_metrics.unsampled_profiles.enabled` | False | 4 | |
| `recovery.disallow-new-enrollment` | False | 0 | *DISK* |
| `relay.drop-transaction-attachments` | False | 1 | |
| `relay.endpoint-fetch-config.enabled` | True | 1 | |
| `release-health.disable-release-last-seen-update` | False | 2 | |
| `release-health.use-org-and-project-filter` | False | 2 | |
| `releases.no_snuba_for_release_creation` | False | 2 | |
| `replay.consumer.enable_new_query_caching_system` | False | 2 | |
| `replay.consumer.msgspec_recording_parser` | False | 1 | |
| `replay.replay-video.disabled` | False | 2 | *DISK* |
| `sdk_http2_experiment.enabled` | False | 1 | |
| `secret-scanning.github.enable-signature-verification` | True | 2 | |
| `seer.explorer_index.enable` | False | 1 | |
| `seer.explorer_index.killswitch.enable` | False | 4 | |
| `seer.global-killswitch.enabled` | False | 2 | |
| `seer.night_shift.enable` | False | 2 | |
| `seer.similarity-embeddings-delete-by-hash-killswitch.enabled` | False | 1 | |
| `seer.similarity-embeddings-killswitch.enabled` | False | 2 | |
| `seer.similarity-killswitch.enabled` | False | 2 | |
| `seer.similarity.ingest.store_hybrid_fingerprint_non_matches` | True | 1 | |
| `seer.supergroups_backfill_lightweight.killswitch` | False | 2 | |
| `sentry-apps.disable-paranoia` | False | 4 | |
| `sentry-apps.disabled-enforcement` | False | 4 | |
| `sentry-apps.hard-delete` | False | 5 | |
| `sentry-apps.webhook.circuit-breaker.dry-run` | False | 1 | |
| `sentry.demo_mode.sync_debug_artifacts.enable` | False | 1 | *DISK* |
| `sentry.send_onboarding_task_metrics` | False | 1 | |
| `sentry.similarity.indexing.enabled` | True | 2 | |
| `sentry:skip-record-onboarding-tasks-if-complete` | False | 1 | |
| `sms.disallow-new-enrollment` | False | 3 | |
| `snuba.search.pre-snuba-candidates-optimizer` | False | 1 | |
| `staff.ga-rollout` | False | 43 | |
| `superuser.read-write.ga-rollout` | False | 20 | |
| `totp.disallow-new-enrollment` | False | 1 | *DISK* |
| `u2f.disallow-new-enrollment` | False | 1 | *DISK* |
| `uptime.automatic-hostname-detection` | True | 3 | |
| `uptime.automatic-subscription-creation` | True | 2 | |
| `uptime.create-issues` | True | 3 | |
| `uptime.use-detectors-by-data-source-cache` | True | 1 | |
| `vsts.social-auth-migration` | False | 1 | |
| `workflow_engine.associate_error_detectors` | False | 2 | |
| `workflow_engine.ensure_detector_association` | True | 2 | |
| `workflow_engine.evaluation_logs_direct_to_sentry` | False | 2 | |

#### Tier 2: Int/Float/String (139 options)

Explicitly-typed numeric and string options. Schema definition is straightforward: `"type": "integer"`, `"type": "number"`, or `"type": "string"`.

Includes: all 22 `outbox_replication.*.replication_version` (Int, default 0), 12 `spans.buffer.*` (Int), 7 `project-abuse-quota.*` (Int, *DISK*), 6 `profiling.*` (Int), 5 `performance.*` Int/Float, 7 `seer.*` (Int), and the 5 String-typed options: `api.deprecation.brownout-cron`, `database.encryption.method`, `dsym.cache-path`, `releasefile.cache-path`, `user-settings.signed-url-confirmation-emails-salt`.

Full list omitted for brevity — 139 options total.

#### Tier 3: Inferred-Simple (196 options)

Options with `inferred` type whose defaults are trivially interpretable: `False` → boolean, `0` / `0.0` / `1.0` → number, `""` → string, `[]` → array, `{}` → object. The type can be derived mechanically from the default value.

Includes: most `backpressure.*`, all `sentry-metrics.writes-limiter.*`, many `performance.issues.*` thresholds, `snuba.search.recommended.*` weights, `relay.*` toggles, `mail.*`, `hybridcloud.*`.

Full list omitted for brevity — 196 options total.

#### Tier 4: Inferred-Complex (77 options)

Options with `inferred` type whose defaults need investigation: URL strings (`"api.github.com"`), JSON objects (`{"url": "http://127.0.0.1:7901"}`), large numbers that could be Int or Float (`500000` vs `50.0`), empty strings for client IDs that need to be typed, and complex list-of-dict defaults (the `sentry-metrics.cardinality-limiter.*` options).

Notable inclusions:
- Integration client IDs: `github-app.client-id`, `slack.client-id`, `discord.application-id`, etc. (empty string defaults, all *DISK*)
- URL configs: `chart-rendering.chartcuterie`, `symbolicator.options`, `symbolserver.options` (JSON object defaults)
- System strings: `system.internal-url-prefix`, `system.security-email`, `system.support-email`, `mail.subject-prefix`
- Large numbers: `performance.issues.render_blocking_assets.size_threshold` (500000), `issues.sdk_crash_detection.cocoa.project_id` (4505469596663808)

Full list omitted for brevity — 77 options total.

#### Tier 5: Sequence/Dict/Any (76 options)

Collection types requiring the most complex schema definitions. `Sequence` options need `"type": "array"` with `"items"` specifying the element type. `Dict` options need `"type": "object"` with `"properties"`. `Any` options need type investigation to determine what they actually are.

Notable groups:
- **Allowlists/denylists** (Sequence, `[]` default): `issues.sdk_crash_detection.*.organization_allowlist`, `replay.viewed-by.project-denylist`, `spans.drop-in-buffer`, `staff.user-email-allowlist`
- **Rate-limit/circuit-breaker configs** (Dict): `apigateway.proxy.circuit-breaker.config`, `seer.similarity.circuit-breaker-config`, `issues.occurrence-consumer.rate-limit.quota`
- **Rollout configs** (Dict): `notifications.platform-rollout.*`, `store.load-shed-process-event-projects-gradual`
- **Load-shed lists** (Any, `[]` default): `store.load-shed-*` — 6 options typed as `Any` that need investigation
- **Structured configs** (Dict): `sentry-apps.webhook.circuit-breaker.config`, `sentry-apps.webhook-logging.enabled`, `uptime.checker-regions-mode-override`

| Option | Type | Default (truncated) | Usages | DISK |
|--------|------|---------------------|--------|------|
| `api.organization.disable-last-deploys` | Sequence | `[]` | 3 | |
| `apigateway.proxy.circuit-breaker.config` | Dict | `{"error_limit": 100...}` | 1 | |
| `autopilot.missing-sdk-integration.projects-allowlist` | Sequence | `[]` | 2 | |
| `autopilot.organization-allowlist` | Sequence | `[]` | 2 | |
| `autopilot.trace-instrumentation.projects-allowlist` | Sequence | `[]` | 2 | |
| `chart-rendering.storage.options` | Dict | `None` | 3 | *DISK* |
| `consumer.dump_stacktrace_on_shutdown` | Sequence | `[]` | 1 | |
| `consumer.verbose_multiprocessing_logs` | Sequence | `[]` | 1 | |
| `crons.organization.disable-check-in` | Sequence | `[]` | 3 | |
| `dashboards.prebuilt-dashboard-ids` | Sequence | `[]` | 2 | |
| `dynamic-sampling.measure.spans` | Sequence | `[]` | 6 | |
| `feedback.organizations.slug-denylist` | Sequence | `[]` | 2 | |
| `github-app.rate-limit-sensitive-orgs` | Sequence | `[]` | 2 | |
| `github-enterprise-app.allowed-hosts-legacy-webhooks` | Sequence | `[]` | 2 | |
| `github-login.extended-permissions` | Sequence | `[]` | 2 | *DISK* |
| `hybrid_cloud.audit_log_event_id_invalid_pass_list` | Sequence | `[]` | 2 | |
| `hybrid_cloud.authentication.disabled_organization_shards` | Sequence | `[]` | 3 | |
| `hybrid_cloud.authentication.disabled_user_shards` | Sequence | `[]` | 3 | |
| `hybridcloud.webhookpayload.skip_on_failure_providers` | Sequence | `["github"]` | 1 | |
| `issue-detection.llm-detection.traces-per-invocation` | Dict | `{"team": 1...}` | 2 | |
| `issue-detection.web-vitals-detection.projects-allowlist` | Sequence | `[]` | 1 | |
| `issues.client_error_sampling.project_allowlist` | Sequence | `[]` | 9 | |
| `issues.occurrence-consumer.rate-limit.quota` | Dict | `{"window_seconds": 3600...}` | 2 | |
| `issues.sdk_crash_detection.dart.organization_allowlist` | Sequence | `[]` | 2 | |
| `issues.sdk_crash_detection.dotnet.organization_allowlist` | Sequence | `[]` | 2 | |
| `issues.sdk_crash_detection.java.organization_allowlist` | Sequence | `[]` | 3 | |
| `issues.sdk_crash_detection.native.organization_allowlist` | Sequence | `[]` | 2 | |
| `issues.sdk_crash_detection.react-native.organization_allowlist` | Sequence | `[]` | 4 | |
| `issues.severity.seer-circuit-breaker-passthrough-limit` | Dict | `{"limit": 1...}` | 1 | |
| `issues.severity.seer-global-rate-limit` | Any | `{"limit": 20...}` | 1 | |
| `issues.severity.seer-project-rate-limit` | Any | `{"limit": 5...}` | 1 | |
| `issues.severity.skip-seer-requests` | Sequence | `[]` | 4 | |
| `notifications.platform-rollout.early-adopter` | Dict | `{}` | 1 | |
| `notifications.platform-rollout.general-access` | Dict | `{}` | 1 | |
| `notifications.platform-rollout.internal-testing` | Dict | `{}` | 6 | |
| `notifications.platform-rollout.is-sentry` | Dict | `{}` | 2 | |
| `notifications.platform.killswitch.sources` | Sequence | `[]` | 1 | |
| `post_process.get-autoassign-owners` | Sequence | `[]` | 2 | |
| `profiling.killswitch.ingest-profiles` | Sequence | `[]` | 3 | *DISK* |
| `profiling.profile_metrics.unsampled_profiles.platforms` | Sequence | `[]` | 2 | |
| `provision_organization.override.mapping` | Dict | `{}` | 2 | |
| `relocation.outbox-orgslug.killswitch` | Sequence | `[]` | 1 | |
| `replay.replay-video.slug-denylist` | Sequence | `[]` | 2 | *DISK* |
| `replay.storage.options` | Dict | `None` | 3 | *DISK* |
| `replay.viewed-by.project-denylist` | Sequence | `[]` | 4 | *DISK* |
| `seer.code-review.excluded-pr-author-logins` | Sequence | `[]` | 2 | |
| `seer.organizations.force-config-reminder` | Sequence | `[]` | 2 | |
| `seer.similarity.circuit-breaker-config` | Dict | `{"error_limit": 33250...}` | 2 | |
| `seer.similarity.global-rate-limit` | Dict | `{"limit": 20...}` | 1 | |
| `seer.similarity.grouping_killswitch_projects` | Sequence | `[]` | 3 | |
| `seer.similarity.per-project-rate-limit` | Dict | `{"limit": 5...}` | 1 | |
| `sentry-apps.expanded-webhook-categories` | Sequence | `[1...]` | 1 | |
| `sentry-apps.webhook-logging.enabled` | Dict | `{"sentry_app_slug": []...}` | 2 | |
| `sentry-apps.webhook.circuit-breaker.config` | Dict | `{"error_limit_window": 600...}` | 2 | |
| `sentry-apps.webhook.restricted-webhook-sending` | Sequence | `[]` | 3 | |
| `snuba.search.recommended.group-type-boost` | Dict | `{7001: 0.15}` | 2 | |
| `spans.buffer.debug-traces` | Sequence | `[]` | 3 | |
| `spans.drop-in-buffer` | Sequence | `[]` | 3 | *DISK* |
| `spans.process-segments.drop-segments` | Sequence | `[]` | 3 | |
| `spans.process-segments.skip-enrichment-projects` | Sequence | `[]` | 2 | |
| `staff.user-email-allowlist` | Sequence | `[]` | 1 | |
| `store.load-shed-group-creation-projects` | Any | `[]` | 3 | |
| `store.load-shed-parsed-pipeline-projects` | Any | `[]` | 2 | |
| `store.load-shed-pipeline-projects` | Any | `[]` | 3 | |
| `store.load-shed-process-event-projects` | Any | `[]` | 3 | |
| `store.load-shed-process-event-projects-gradual` | Dict | `{}` | 2 | |
| `store.load-shed-save-event-projects` | Any | `[]` | 2 | |
| `store.load-shed-symbolicate-event-projects` | Any | `[]` | 3 | |
| `symbolicator.ignored_sources` | Sequence | `[]` | 2 | |
| `tempest.tempest-ips-api-response` | Sequence | `[]` | 2 | |
| `u2f.facets` | Sequence | `[]` | 4 | *DISK* |
| `uptime.checker-regions-mode-override` | Dict | `{}` | 6 | |
| `uptime.restrict-issue-creation-by-hosting-provider-id` | Sequence | `[]` | 2 | |
| `uptime.uptime-ips-api-response` | Sequence | `[]` | 2 | |
| `workflow_engine.group.type_id.disable_issue_stream_detector` | Sequence | `[8001]` | 2 | |
| `workflow_engine.group.type_id.open_periods_type_denylist` | Sequence | `[]` | 1 | |

---

## Comparison

| | Proposal A (Risk) | Proposal B (Namespace) | Proposal C (Type) |
|---|---|---|---|
| **Best for** | Minimizing blast radius | Team coordination | Schema tooling validation |
| **First tier size** | 36 (free wins) | 85 (infra plumbing) | 68 (booleans) |
| **Biggest tier** | 191 (Tier 2) | 314 (Tier 2) | 196 (Tier 3) |
| **DISK handling** | Scattered across all tiers | Clustered by namespace | Scattered across all tiers |
| **Cross-team impact** | Every tier touches many teams | One domain per tier | Every tier touches many teams |
| **Complexity ramp** | Gradual (by call-site count) | Moderate (by domain risk) | Gradual (by type complexity) |
| **Schema complexity** | Mixed in every tier | Mixed in every tier | Incremental by design |

## Cross-cutting concerns

These apply regardless of which ordering is chosen:

- **110 `FLAG_PRIORITIZE_DISK` options** need special migration handling — the new system reads from ConfigMaps (disk), so behavior should be preserved, but the fallback chain differs
- **273 `inferred` type options** need their actual types determined from default values and code before schema creation
- **8 `Any`-typed options** (`store.load-shed-*`, `issues.severity.seer-*-rate-limit`) need type investigation
- **52 options used in both sentry and getsentry** need cross-repo verification
- **Production database check is mandatory** for every option before migration — a non-default DB value means the option is actively configured
- **Value mirroring** between old and new systems should be set up before any migration begins (per the Notion migration plan)
