# Working with options

Day-to-day tasks once your service is [set up](./setup.md). For the schema type reference, see [Architecture → Schema Format](./architecture.md#schema-format).

## Reading options

```python
from sentry_options import init, options

init()
opts = options('seer')

timeout = opts.get('inference.timeout')          # str | int | float | bool
allowed = opts.get('inference.allowed_models')   # list
db = opts.get('database.config')                 # dict
```

```rust
use sentry_options::{init, options};

init()?;
let opts = options("seer");
let timeout = opts.get("inference.timeout")?;    // serde_json::Value
```

An option returns its schema default when no value is set, so code can ship before any values exist.

## Adding an option

1. Add it to `sentry-options/schemas/{namespace}/schema.json`:
   ```json
   "inference.timeout": {
     "type": "integer",
     "default": 100,
     "description": "Inference timeout in seconds"
   }
   ```
   See [Architecture → Schema Format](./architecture.md#schema-format) for all supported types.
2. Open a PR in the service repo — CI validates the change is additive (see [evolution rules](#schema-evolution-rules)).
3. Merge. The option uses its schema default until you set a value.
4. (Optional) set a non-default value in the automator — see [Setting values](#setting-values).

## Deleting an option

1. Remove it from the schema.
2. CI warns if the option is still referenced in sentry-options-automator values — remove those values first.
3. Removing an entire namespace is not allowed (see [evolution rules](#schema-evolution-rules)).

## Setting values

Values live in **sentry-options-automator**, not the service repo.

`option-values/{namespace}/default/values.yaml` holds base values inherited by all targets:
```yaml
options:
  inference.timeout: 200
```

Region overrides go in `option-values/{namespace}/{region}/values.yaml` and are merged on top of `default/`. Each namespace/target produces a ConfigMap named `sentry-options-{namespace}`.

Merge the automator PR — CD deploys the updated ConfigMaps. Changes hot-reload without a pod restart (see [below](#hot-reload-behavior)).

## Schema evolution rules

CI enforces these on every schema PR:

| Change | Allowed |
|--------|---------|
| Add new options | ✅ |
| Add new namespaces | ✅ |
| Remove options | ✅ (CI warns if still used in automator) |
| Remove namespaces | ❌ |
| Change option types | ❌ |
| Change default values | ❌ |

Breaking changes need a migration strategy — contact DevInfra.

## Feature flags

Options prefixed `feature.` are flagpole-style flags evaluated against a context. See the [flagpole docs](https://develop.sentry.dev/backend/application-domains/feature-flags/flagpole/) for segment/condition semantics.

```python
from sentry_options import init, features, FeatureContext

init()
context = FeatureContext(
    {"org_id": 123, "user_id": 456, "user_email": "sal@example.org"},
    identity_fields=["user_id"],
)

if features("seer").has("organizations:red-bar", context):
    ...
```

```rust
use sentry_options::{init, features, FeatureContext};

init()?;
let mut context = FeatureContext::new();
context.insert("user_id", 123);
context.insert("org_id", 456);
context.identity_fields(vec!["user_id"]);

if features("seer").has("organizations:red-bar", &context) {
    // ...
}
```

## Testing locally

The client looks for a `sentry-options/` directory in your working directory. Drop values in `sentry-options/values/{namespace}/values.json` (local testing uses JSON, not YAML):

```json
{
  "options": {
    "inference.timeout": 30,
    "processing.enabled": true
  }
}
```

```
sentry-options/
├── schemas/{namespace}/schema.json
└── values/{namespace}/values.json
```

Edit `values.json` while the service runs to watch hot-reload (~5s).

## Testing with overrides

`override_options` temporarily replaces values in tests. Overrides are validated against the schema (unknown keys and type mismatches raise) and are thread-local — they won't apply to spawned threads.

Python — requires `init()` first, via a `conftest.py` fixture:
```python
# conftest.py
import pytest
from sentry_options import init

@pytest.fixture(scope='session', autouse=True)
def _init_options() -> None:
    init()
```
```python
from sentry_options import options
from sentry_options.testing import override_options

def test_timeout():
    with override_options('seer', {'inference.timeout': 30}):
        assert options('seer').get('inference.timeout') == 30
```
Nesting is supported — inner overrides restore to outer values on exit.

Rust — `init()` is idempotent; the guard restores values when dropped:
```rust
use sentry_options::testing::override_options;
use sentry_options::{init, options};
use serde_json::json;

#[test]
fn test_timeout() {
    init().unwrap();
    let _guard = override_options(&[
        ("seer", "inference.timeout", json!(30)),
    ]).unwrap();
    assert_eq!(options("seer").get("inference.timeout").unwrap(), json!(30));
}
```

## Hot-reload behavior

- Polling interval: ~5 seconds
- ConfigMap propagation: ~1–2 minutes (kubelet sync period)
- Total latency: ConfigMap update → ~1–2 min → file update → ~5s → reload

No pod restart required when values change.
