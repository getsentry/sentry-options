# Experiments

An experiment splits subjects into weighted arms so you can compare behaviours. It is an `experiment.`-prefixed option: declared in the schema, configured in values, read at runtime through `experiments(namespace)`. Feature flags and rollouts are unchanged; experiments are a separate construct.

Assignment is deterministic and publicly reproducible from the namespace, names and unit values. Never use it to gate anything security-sensitive.

## Concepts

- **Unit** is the identity an experiment attaches to: a list of context fields such as `["run_id"]` or `["organization_id", "pr_id"]`. The same unit values always get the same result.
- **Layer** groups experiments that must not overlap. A layer has 100 slots. Each experiment claims a contiguous **allocation** of slots; a subject is hashed once per layer to a slot, and only the experiment owning that slot sees it. Slots nobody claims are **holdout**. Experiments in different layers are hashed independently, so every subject is in both.
- **Arms** are weighted. The arm is chosen by a second hash that is independent of the slot, so shrinking an allocation drops subjects without reshuffling the ones who stay, and changing weights never changes who is in the experiment.

Every experiment belongs to exactly one layer. A standalone experiment is a layer with one member. Put experiments that could interfere with each other in the same layer; put unrelated ones in different layers so each gets all the traffic.

## Adding an experiment

In `sentry-options/schemas/{namespace}/schema.json`:

```json
"experiment.checkout-color": {
  "$ref": "#/definitions/Experiment"
}
```

or, to declare experiments by pattern, `"patternProperties": { "^experiment\\.": { "$ref": "#/definitions/Experiment" } }`. Like feature flags, experiment keys take no `type` or `default`, and they are exempt from schema-evolution checks.

## Setting an experiment value

In the automator's `option-values/{namespace}/{target}/values.yaml`:

```yaml
experiment.checkout-color:
  owner: { team: growth }
  description: Green vs blue checkout button
  layer: checkout
  unit: [organization_id]
  allocation: { start: 0, size: 40 }
  arms:
    - { name: control, weight: 50 }
    - { name: treatment, weight: 50, config: { color: green } }
```

| Field | Meaning |
| --- | --- |
| `layer` | Experiments sharing a layer are mutually exclusive. Rename the layer to reshuffle everyone in it. |
| `unit` | Context fields that identify the subject. Every experiment in a layer must declare the same unit. |
| `allocation` | `start` is the first slot (0-99), `size` the number of slots, i.e. percent of the layer. `{start: 0, size: 40}` covers slots 0 through 39. |
| `arms` | One or more `{name, weight, config?}`. Weights are relative integers up to 1,000,000,000; `config` is free-form JSON your code reads. |
| `enabled` | Optional, default `true`. When `false`, subjects in the allocation are reported as `disabled` and get no arm. The allocation stays reserved. |
| `owner`, `description`, `created_at` | Same shape as on feature flags; only `owner.team` is required. |

Validation rejects allocations that run past slot 99, overlapping allocations in a layer, differing units in a layer, duplicate arm names, and an enabled experiment whose weights sum to 0 (a paused one may zero them). The message names the experiments involved.

### Sharing a layer

Slots are claimed explicitly, so adding a second experiment to a running layer means picking free slots:

```yaml
experiment.checkout-color:
  allocation: { start: 0, size: 40 }    # slots 0-39
experiment.checkout-copy:
  allocation: { start: 40, size: 30 }   # slots 40-69; 70-99 stay holdout
```

If the layer is full, either shrink a running experiment (safe: remaining subjects keep their arms) or, if the two experiments are unrelated, move the new one to its own layer.

To end an experiment, remove it; its slots become holdout. For long-lived units like organizations, the next experiment placed on those slots inherits a cohort that was just treated. Rename the layer (for example `checkout-v2`) when that matters.

## Reading an experiment

Python:

```python
from sentry_options import experiments

assignment = experiments("seer").assign("checkout-color", {"organization_id": org_id})
if assignment.in_arm("treatment"):
    color = assignment.config["color"]
```

Rust:

```rust
use sentry_options::{experiments, ExperimentContext};

let context = ExperimentContext::from([("organization_id".to_string(), org_id.into())]);
let assignment = experiments("seer").assign("checkout-color", &context);
if assignment.in_arm("treatment") { /* ... */ }
```

`assign` never raises; `try_assign` raises `ExperimentError` (or the usual options errors) instead of returning an `unassigned` result. In Python, a context that cannot be converted (a non-string key, `NaN`) comes back as `unassigned` from `assign`, while `try_assign` raises the conversion error (`ValueError`). The context is a plain mapping; only the fields named in `unit` are read. String values are used verbatim, numbers and booleans are rendered as JSON (`123`, `1.5`, `true`).

An `Assignment` carries:

| Field | Value |
| --- | --- |
| `status` | `assigned`, `excluded` (slot owned by another experiment in the layer), `holdout` (slot free), `disabled` (in the allocation but `enabled: false`), `unassigned` (not configured, not initialized, or a missing unit field; see `reason`) |
| `arm`, `config` | Set only when `assigned` |
| `slot`, `layer`, `unit`, `subject` | Where the subject landed; `subject` is the unit values joined with `:` |
| `excluded_by` | The owning experiment when `excluded` |

`experiments(ns).layer("checkout")` lists a layer's members with their allocations, useful for tooling and tests.

## Exposure

The library decides assignments; it does not record them. Log an exposure when the subject actually receives the treatment, once per subject per experiment, using `assignment.to_dict()` (Python) or `assignment.to_json()` (Rust). It is one flat record with stable keys: `namespace, experiment, layer, unit, subject, slot, status, arm, excluded_by, reason`. `unit` is the list of unit fields; `subject` is those values joined with `:`. `config` is left out on purpose. Checking an assignment is free and silent; only the log call marks an exposure.

Eligibility is not part of the experiment. Gate with a feature flag first, then assign.

## Testing

```python
from sentry_options.testing import experiment, override_options

with override_options("seer", {
    "experiment.checkout-color": experiment(
        layer="checkout", unit=["organization_id"], arms={"control": 0, "treatment": 100},
        start=70, size=30,
    ),
}):
    assert experiments("seer").assign("checkout-color", {"organization_id": 1}).arm == "treatment"
```

`experiment()` builds a valid value with `allocation {start: 0, size: 100}` and `enabled: true` unless told otherwise. The overrides in one `override_options` call are checked together against the layer when they are set, so an overlapping batch fails right there and nothing is applied; give each free slots or a layer name nothing else uses. Overrides are validated against the `Experiment` shape and bypass the assignment cache, so they take effect immediately.

## How the hash works

Both hashes are SHA-1 over a list of components, each written as its byte length (8 bytes, big-endian) followed by its bytes; the first 8 digest bytes, read big-endian, are reduced modulo the range.

- slot = hash(`"layer"`, namespace, layer, unit values...) mod 100
- arm point = hash(`"experiment"`, namespace, experiment, unit values...) mod total weight; arms are walked in order, each taking `weight` points.

Reference implementation:

```python
import hashlib

def bucket(components: list[str], modulus: int) -> int:
    h = hashlib.sha1()
    for c in components:
        b = c.encode()
        h.update(len(b).to_bytes(8, "big"))
        h.update(b)
    return int.from_bytes(h.digest()[:8], "big") % modulus
```

The length prefix keeps names containing `:` from colliding. This hash is separate from the one feature flags use for rollouts, which stays byte-for-byte unchanged.
