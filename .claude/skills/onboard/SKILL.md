---
name: onboard
description: "Onboard a service to sentry-options. Use when integrating a new service, creating schemas, or setting up the multi-repo sentry-options workflow."
---

# sentry-options Onboarding

Guide the user through integrating a service with sentry-options. The authoritative, step-by-step instructions live in the setup guide — **follow it rather than reproducing details here.** This skill intentionally defers specifics (version pins, exact file contents, schema rules) to the docs so it can't drift out of sync.

**Setup guide:** https://github.com/getsentry/sentry-options/blob/main/docs/setup.md
**Options/schema reference:** https://github.com/getsentry/sentry-options/blob/main/docs/options.md

## How to run this

1. **Gather inputs** from the user:
   - Repository name (e.g. `seer`, `snuba`).
   - Namespace(s) — must be the repo name or prefixed with `{repo}-` (e.g. `seer`, `seer-autofix`).
   - Any initial options or feature flags (name, type, default, description).

2. **Walk the three phases from the setup guide, in order**, using the exact file contents, version pins, and commands it specifies (do not write versions or schema rules from memory — they change):
   - **Phase 1 — service repo:** create `sentry-options/schemas/{namespace}/schema.json`, add the `validate-schema` CI workflow and (for Python) the `sentry-options-gen-stubs` pre-commit hook, add the client dependency, call `init()` at startup, and copy schemas into the image with `SENTRY_OPTIONS_DIR`.
   - **Phase 2 — sentry-options-automator:** register the repo in `repos.json` and add `option-values/{namespace}/{target}/values.yaml` (the mandatory `default` target plus a folder per region you want to deploy to).
   - **Phase 3 — ops:** add the injector annotations (`options.sentry.io/inject`, `options.sentry.io/namespace`) to the deployment.

3. For schema/type details (primitives, arrays, structs, dicts, optional fields, feature flags) consult the options/schema reference.

4. After each phase, surface the checkpoint from the guide (e.g. merge the PR) before moving on.
