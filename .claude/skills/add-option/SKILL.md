---
name: add-option
description: "Add a new option to an existing sentry-options schema. Use when adding feature flags or tunable parameters to a service already using sentry-options."
---

# Add an Option to a sentry-options Schema

Guide the user through adding an option (or feature flag) to a service that's already onboarded to sentry-options. The authoritative steps and the full type reference live in the options guide — **follow it rather than reproducing details here.** This skill defers specifics (type shapes, evolution rules) to the docs so it can't drift.

**Options guide:** https://github.com/getsentry/sentry-options/blob/main/docs/options.md#adding-an-option

## How to run this

1. **Confirm the service is onboarded** — it should already have `sentry-options/schemas/{namespace}/schema.json`. If not, use `/onboard` first.

2. **Gather the new option** from the user: namespace, name, type (`string`, `integer`, `number`, `boolean`, `array`, `object`, or a `feature.`-prefixed flag), default, and description.

3. **Add it** under `properties` in `sentry-options/schemas/{namespace}/schema.json`. For the exact shape of each type — arrays (`items`), structs (`properties`), dicts (`additionalProperties`), `optional` fields, and feature `$ref`s — read the options guide; don't write type rules from memory.

4. **Tell the user the workflow:** open a PR in the service repo (CI validates the change is additive per the schema-evolution rules), merge it, then optionally set a non-default value in sentry-options-automator. The option uses its schema default until a value is set.

For the schema-evolution rules and how to set values, see the options guide.
