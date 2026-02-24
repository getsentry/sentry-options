---
name: add-option
description: "Add a new option to an existing sentry-options schema. Use when adding feature flags or tunable parameters to a service already using sentry-options."
---

# Add Option to sentry-options Schema

This skill helps you add new options to an existing sentry-options schema.

**Full documentation:** https://github.com/getsentry/sentry-options/blob/main/docs/integration-guide.md

## Prerequisites

The service must already be onboarded to sentry-options (has `sentry-options/schemas/{namespace}/schema.json`). If not, use `/onboard` first.

---

## Step 1: Gather Information

Ask the user for:

1. **Repository/namespace** - Which schema to update (e.g., `seer`, `seer-autofix`)
2. **New option details:**
   - Name (e.g., `feature.new_thing.enabled`)
   - Type: `string`, `integer`, `number`, or `boolean`
   - Default value
   - Description

**Namespace validation:** The namespace must be either `{repo}` (exact match) or `{repo}-*` (prefixed with repo name). For example, in the `seer` repo: `seer`, `seer-autofix` are valid; `autofix` alone is **not valid**.

---

## Step 2: Update Schema

Read the existing `sentry-options/schemas/{namespace}/schema.json` and add the new option to the `properties` object.

**New option format:**
```json
"{option_name}": {
  "type": "{option_type}",
  "default": {option_default},
  "description": "{option_description}"
}
```

**Supported types:**
- `string` - Default must be a quoted string (e.g., `"default_value"`)
- `integer` - Default must be a whole number (e.g., `100`)
- `number` - Default can be integer or float (e.g., `3.14`)
- `boolean` - Default must be `true` or `false`
- `array` - Must include `items` property with element type (e.g., `{"type": "integer"}`). Default must be an array (e.g., `[1, 2, 3]`)

**Example - adding to existing schema:**

Before:
```json
{
  "version": "1.0",
  "type": "object",
  "properties": {
    "existing.option": {
      "type": "boolean",
      "default": false,
      "description": "Existing option"
    }
  }
}
```

After:
```json
{
  "version": "1.0",
  "type": "object",
  "properties": {
    "existing.option": {
      "type": "boolean",
      "default": false,
      "description": "Existing option"
    },
    "new.option": {
      "type": "integer",
      "default": 100,
      "description": "New option description"
    }
  }
}
```

**Example - adding an array option:**
```json
"feature.allowed_ids": {
  "type": "array",
  "items": {"type": "integer"},
  "default": [1, 2, 3],
  "description": "List of allowed IDs"
}
```

---

## Step 3: Schema Evolution Rules

Remind the user of these CI-enforced rules:

| Change | Allowed |
|--------|---------|
| Add new options | Yes |
| Add new namespaces | Yes |
| Remove options | No (coming soon) |
| Remove namespaces | No |
| Change option types | No |
| Change default values | No |

**Important:** Once an option is added, you cannot change its type or default value. Choose carefully.

---

## Step 4: Update Workflow

After updating the schema, tell the user:

> **Next steps:**
>
> 1. **Create PR** in your service repo with the schema change
> 2. **Merge PR** - CI will validate the schema change is additive-only
> 3. **Update sentry-options-automator:**
>    - Update `repos.json` with the new merge commit SHA
>    - Optionally add values for the new option in `option-values/{namespace}/default/values.yaml`
> 4. **Merge automator PR** - CD will deploy updated ConfigMaps
>
> The new option will use its schema default until you add explicit values in the automator.

---

## Optional: Add Values

If the user wants to set a non-default value for the new option:

Generate addition to `option-values/{namespace}/default/values.yaml`:

```yaml
options:
  # ... existing options ...
  {new_option_name}: {new_option_value}
```

**Note:** The `default/` values are inherited by all targets. For target-specific values, add to `option-values/{namespace}/{target}/values.yaml`.

---

## Quick Reference

**Schema location:** `sentry-options/schemas/{namespace}/schema.json`

**Values location:** `option-values/{namespace}/default/values.yaml` (in sentry-options-automator)

**Supported types:** `string`, `integer`, `number`, `boolean`, `array`

**Hot-reload:** Changes propagate in ~1-2 minutes without pod restart
