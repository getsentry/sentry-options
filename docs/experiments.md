# Experiments

An experiment splits subjects into weighted arms so you can compare behaviours. It is declared inside an `experiment-layer.`-prefixed layer block: declared in the schema, configured in values, read at runtime through `experiments(namespace)`. Feature flags and rollouts are unchanged; experiments are a separate construct.

Assignment is deterministic and publicly reproducible from the namespace, names and unit values. Never use it to gate anything security-sensitive.

## Concepts

- **Unit** is the identity an experiment attaches to: a list of context fields such as `["run_id"]` or `["organization_id", "pr_id"]`. The same unit values always get the same result.
- **Layer** groups experiments that must not overlap. A layer has 100 slots. Each experiment claims a contiguous **allocation** of slots; a subject is hashed once per layer to a slot, and only the experiment owning that slot sees it. Slots nobody claims are **holdout**. Experiments in different layers are hashed independently, so every subject is in both.
- **Arms** are weighted. The arm is chosen by a second hash that is independent of the slot, so shrinking an allocation drops subjects without reshuffling the ones who stay, and changing weights never changes who is in the experiment.

Every experiment belongs to exactly one layer. A standalone experiment is a layer with one member. Put experiments that could interfere with each other in the same layer; put unrelated ones in different layers so each gets all the traffic.

## Adding an experiment

Experiments live inside a layer. In `sentry-options/schemas/{namespace}/schema.json`, declare the layer:

```json
"experiment-layer.checkout": {
  "$ref": "#/definitions/ExperimentLayer"
}
```

or, to declare layers by pattern, `"patternProperties": { "^experiment-layer\\.": { "$ref": "#/definitions/ExperimentLayer" } }`. Like feature flags, layer keys take no `type` or `default`, and they are exempt from schema-evolution checks. A standalone experiment is a layer with one member; the layer name may simply repeat the experiment name.

## Setting an experiment value

In the automator's `option-values/{namespace}/{target}/values.yaml`, everything in a layer sits in one block, so you always see a layer's neighbours and their slots before adding to it:

```yaml
experiment-layer.checkout:
  description: Checkout page tests
  unit: [organization_id]
  experiments:
    checkout-color:
      owner: { team: growth }
      description: Green vs blue checkout button
      allocation: { start: 0, size: 40 }
      arms:
        - { name: control, weight: 50 }
        - { name: treatment, weight: 50, config: { color: green } }
    checkout-copy:
      owner: { team: growth }
      allocation: { start: 40 }
      arms:
        - { name: short, weight: 50 }
        - { name: long, weight: 50 }
```

| Field | Meaning |
| --- | --- |
| `unit` (layer) | Context fields that identify the subject. Lives on the layer; every experiment in it shares the unit. |
| `experiments` (layer) | The layer's experiments, keyed by name. At least one; each claims a disjoint allocation. |
| `allocation` | `start` is the first slot (0-99), `size` the number of slots, i.e. percent of the layer. `size` defaults to 20 and may not go below 10. `{start: 0, size: 40}` covers slots 0 through 39. |
| `arms` | 1 to 10 `{name, weight, config?}`. Weights are relative integers up to 1,000,000,000; `config` is free-form JSON your code reads. |
| `enabled` | Optional, default `true`. When `false`, subjects in the allocation are reported as `disabled` and get no arm. The allocation stays reserved. |
| `owner`, `description`, `created_at` | Same shape as on feature flags; only `owner.team` is required. |

The default allocation is 20 slots because that is what reaches significance on current Autofix traffic in about a week. Rough sample size per arm is `n ≈ 16 × p × (1 − p) / effect²`; at a 30% baseline a 5-point lift needs about 1,350 subjects per arm. In the week of Sep 9 2026 Autofix ran about 1,450 runs and 290 orgs a day, so a two-arm, run-unit experiment on 20% of a layer collects roughly 145 runs per arm per day and clears 1,350 in about nine days. Take the default and leave the rest of the layer free for the next experiment; raise `size` only when you need the traffic sooner and can spare the slots, and never drop below the floor of 10.

Validation rejects allocations that run past slot 99, allocations smaller than 10, overlapping allocations in a layer, an experiment name declared in two layers, duplicate arm names, and an enabled experiment whose weights sum to 0 (a paused one may zero them). The overlap message names the two experiments, their slot ranges, and the layer's free slots.

### Sharing a layer

Because the whole layer is one block, adding a second experiment means editing the block and picking free slots the validator prints for you:

```yaml
experiment-layer.checkout:
  unit: [organization_id]
  experiments:
    checkout-color:
      allocation: { start: 0, size: 40 }    # slots 0-39
    checkout-copy:
      allocation: { start: 40, size: 30 }   # slots 40-69; 70-99 stay holdout
```

If the layer is full, either shrink a running experiment (safe: remaining subjects keep their arms) or, if the two experiments are unrelated, give the new one its own layer.

To end an experiment, remove it from the layer's `experiments`; its slots become holdout. For long-lived units like organizations, the next experiment placed on those slots inherits a cohort that was just treated. Rename the layer (for example `experiment-layer.checkout-v2`) when that matters.

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

`assign` never raises; `try_assign` raises `ExperimentError` (or the usual options errors) instead of returning an `unassigned` result. In Python, a context that cannot be converted (a non-string key, `NaN`) comes back as `unassigned` from `assign`, while `try_assign` raises the conversion error: a non-string key raises `TypeError`, and `NaN`/`Infinity` raise `ValueError`. The context is a plain mapping; only the fields named in `unit` are read. String values are used verbatim, numbers and booleans are rendered as JSON (`123`, `1.5`, `true`).

An `Assignment` carries:

| Field | Value |
| --- | --- |
| `status` | `assigned`, `excluded` (slot owned by another experiment in the layer), `holdout` (slot free), `disabled` (in the allocation but `enabled: false`), `unassigned` (not configured, not initialized, or a missing unit field; see `reason`) |
| `arm`, `config` | Set only when `assigned` |
| `slot`, `layer`, `unit`, `subject` | Where the subject landed; `subject` is the JSON array of the unit values, e.g. `["1","77"]`, so it stays one column and cannot collide |
| `excluded_by` | The owning experiment when `excluded` |

`experiments(ns).layer("checkout")` lists a layer's members with their allocations, useful for tooling and tests.

## Exposure

The library decides assignments; it does not record them. Sentry and Seer each send rows to their own sink, so the row has to carry the same shape and the same meaning from both. Build it with `assignment.exposure("seer")` (`Assignment.exposure(service)`, Python and Rust alike) rather than assembling it by hand. It is `to_dict()` (Python) or `to_json()` (Rust) plus three keys: `record_version`, `service`, and `recorded_at` (UTC, RFC 3339; stamped with the current time). The full record is `namespace, experiment, layer, unit, subject, slot, allocation_start, allocation_size, definition_revision, status, arm, excluded_by, reason, record_version, service, recorded_at`, where `unit` is the list of unit fields and `subject` is the JSON array of the unit values, so it stays one column and cannot collide. `definition_revision` is a short content hash of the experiment definition that changes whenever the layer, unit, allocation, arms, their weights or config, or `enabled` changes, so the warehouse can split an analysis at a revision boundary. `config` is left out on purpose.

**Only `assigned` rows are exposures.** The analysis entry for a subject is the first row with status `assigned` per `(namespace, experiment, subject, definition_revision)`, where `subject` is the JSON array of the unit values, so two composite subjects never collide into one key. Later `assigned` rows for the same subject are ignored, and a row with any other status is never counted as an exposure.

**Log at the trigger, for every arm.** Write the row at the point in the code where the arm changes behaviour, and write it there for every arm, control included, on the same code line. Do not write it when `assign` is called: being assigned is not being exposed. This symmetry is the whole point. If control were logged somewhere else, or not at all, the arms would be counted under different conditions and the comparison would already be biased before analysis starts.

```python
assignment = experiments("seer").assign("gemini-high", {"run_id": run.id})
exposures.emit(assignment.exposure("seer"))
if assignment.is_assigned:
    model = assignment.config["model"]
```

**Compare arm to arm.** The primary comparison is between the arms of the experiment. The layer holdout is a layer-level baseline — what the layer as a whole does to its subjects — not the control arm of any one experiment.

**Keep writing decision records.** The same helper at the same trigger also writes `holdout`, `excluded`, and `disabled` rows, tagged by `status`. They are not exposures; they exist for auditing — proving mutual exclusion inside a layer, checking that holdout and allocation sizes match the configuration, confirming a paused experiment really was paused, and debugging why a given subject saw no treatment. `unassigned` is never written: an unknown experiment or missing context is an application bug, so log it to your error tracker instead.

Dedupe before writing is an optimization, not part of the contract, because the analysis takes the first `assigned` row anyway. A service that dedupes (Sentry's 24h window) and one that does not (Seer, one row per run) reach the same result, as long as dedupe stays within a `definition_revision`: once the definition changes, the next `assigned` row is a distinct entry and must be written. `record_version` bumps only when a field is renamed, removed, or changes meaning; adding fields does not bump it, and warehouse models should key on it. The library never sends anything; the send (BigQuery, Amplitude, a Sentry span, a log line) belongs to the service.

Eligibility is not part of the experiment. Gate with a feature flag first, then assign.

## Changing an allocation mid-experiment

An allocation sometimes has to change while an experiment runs: shrinking one to free slots for a sibling in the same layer, or widening a winner to more traffic. Assignment stays stable across the change because the slot depends only on layer and unit, never on the allocation, and the arm depends only on the experiment and unit, never on membership. A subject whose slot stays inside the allocation keeps both its slot and its arm; only subjects whose slot leaves the allocation change status.

Take a shrink from `{start 0, size 50}` to `{start 0, size 33}`, moving the experiment from slots 0-49 to slots 0-32:

| Slot range | Before | After | Cohort |
| --- | --- | --- | --- |
| 0-32 | in | in | stayed in for the whole window |
| 33-49 | in | out | fell out at the change |
| 50-99 | out | out | never in this experiment |

To analyze a window that spans the change, restrict it to the slots inside both the old and the new allocation (their intersection, slots 0-32 here) for a cohort with one consistent experience. Slots that fell out (33-49) are a separate cohort: analyze them in the pre-change window only, or drop them. Growth is the mirror image: newly added slots have a shorter history, so either start their clock at the change or restrict the analysis to the original slots.

Because `allocation_start` and `allocation_size` ride on every row, exposures and decision records alike, the cutover is visible directly in the data, with no manual join against the values repo history. The change also bumps `definition_revision` — allocation is part of the hash — so the windows on either side are distinct revisions even though no subject's slot or arm moved, and this intersection is how you read one cohort across a pure grow. Prefer changing `size` and leaving `start` where it is: moving `start` reshuffles nobody but shifts the intersection and so shrinks the cohort you can analyze across the change.

## Assignment lifetime

**Assignment is per call, not stored.** Mutual exclusion inside a layer holds per subject, per namespace, per values snapshot, per call. `assign` is a pure function of the subject and the current definition; the library stores nothing, so a later call can answer differently if the definition changed in between.

**Ramp up, never ramp down.** Within a phase you can grow the allocation or pause it (`enabled: false`). Shrinking the allocation, or changing arms, weights, unit, or layer, ends the phase: the `definition_revision` changes and the new revision is analyzed as a new experiment. A grow changes the revision too — allocation is part of the hash — so analysis restarts at each ramp unless the analyst deliberately reads across a pure grow using the slot intersection from "Changing an allocation mid-experiment". Never shrink expecting old subjects to be kept.

**Call `assign` once per subject, at the trigger.** For a unit that occurs once (a run id) that is the whole story: one trigger, one call, one row, nothing to persist and no first-time check. Later steps in the same run that need the arm should receive it from the trigger rather than re-calling `assign`. A unit that recurs across calls (an org id, a user id) gets the same answer as long as the definition has not changed; keeping such subjects stable across a shrink would need a persisted per-subject decision, which the library does not provide and which is out of scope for now.

**A kill switch is a separate flag.** To stop the treatment behaviour for subjects already exposed, gate the behaviour with a feature flag checked at the trigger. `enabled: false` pauses assignment — new calls return `disabled` — but it is not a behaviour kill switch and should not be used as one.

## Ending an experiment

Two ways to stop. Pause with `enabled: false` to stop reversibly: the allocation stays reserved and callers in it come back `disabled`. Delete the experiment from its `experiment-layer.<layer>` block (and the layer key once its last experiment is gone) to end it and free the slots. Pause is not a behaviour kill switch, though: it stops new assignments, not treatment already running, so to turn treatment off for subjects mid-flight, gate the behaviour with a feature flag (see "Assignment lifetime").

To end one cleanly:

- Ship the winner the ordinary way, as a plain option, feature flag, or code, so the behaviour no longer depends on the experiment.
- Delete the experiment from its `experiment-layer.<layer>` block (and the layer key once its last experiment is gone) in the values and the schema, values first or together. Experiment keys are exempt from the schema-evolution checks, so no deprecation dance is needed.
- The layer's slots free up for the next experiment once the value is gone.

The moment the value is gone, `assign` returns `unassigned` with reason `"experiment is not configured"`, `is_assigned` is `false` and `config` is `None`, so code branching on `is_assigned` falls through to its default path with no deploy. Exposure stops too: an `unassigned` result is never written, let alone counted.

To hand everyone the winner today but delete the code next week, keep the experiment and make the winner's arm the only arm (or weight the loser `0`). Everyone in the allocation gets its config, but it still blocks siblings in the layer, so do not park it there.

Never reuse an experiment name for a different test. Both hashes are seeded by namespace and name, so a fresh experiment reusing a name and unit reproduces the old assignment exactly and inherits its imbalance. Pick a new name; the layer can stay.

In the warehouse, the ended experiment's rows stay valid: the analysis window ends at the deletion, and each row's `allocation_start` and `allocation_size` already say what was live.

## Testing

```python
from sentry_options.testing import experiment, experiment_layer, override_options

with override_options("seer", {
    "experiment-layer.checkout": experiment_layer(
        unit=["organization_id"],
        experiments={
            "checkout-color": experiment(arms={"control": 0, "treatment": 100}, start=70, size=30),
        },
    ),
}):
    assert experiments("seer").assign("checkout-color", {"organization_id": 1}).arm == "treatment"
```

`experiment()` builds a valid experiment with `allocation {start: 0, size: 20}` and `enabled: true` unless told otherwise, and `experiment_layer()` wraps one or more into a layer. The overrides in one `override_options` call are checked together against the layer when they are set, so an overlapping batch fails right there and nothing is applied; give each free slots or a layer name nothing else uses. Overrides are validated against the `Experiment` shape and bypass the assignment cache, so they take effect immediately.

## How the hash works

Both hashes are SHA-1 over a list of components, each written as its byte length (8 bytes, big-endian) followed by its bytes; the first 8 digest bytes are read big-endian as a 64-bit `point`.

- slot = `point`(`"layer"`, namespace, layer, unit values...) mod 100.
- arm = `point`(`"experiment"`, namespace, experiment, unit values...) placed on the fixed `0..2^64` domain. Walking arms in declared order and accumulating their weights, the first arm whose running total `cum` satisfies `point / 2^64 < cum / total` owns the subject. The point never depends on the weights, so scaling every weight by the same factor (50:50 to 1:1) reassigns nobody; reordering arms or inserting one does reshuffle.

Reference implementation:

```python
import hashlib

def point(components: list[str]) -> int:
    h = hashlib.sha1()
    for c in components:
        b = c.encode()
        h.update(len(b).to_bytes(8, "big"))
        h.update(b)
    return int.from_bytes(h.digest()[:8], "big")

def slot(components: list[str]) -> int:
    return point(components) % 100

def arm(components: list[str], arms: list[tuple[str, int]]) -> str | None:
    total = sum(weight for _, weight in arms)
    p = point(components)
    cum = 0
    for name, weight in arms:
        cum += weight
        if p * total < (cum << 64):
            return name
    return None
```

The length prefix keeps names containing `:` from colliding. This hash is separate from the one feature flags use for rollouts, which stays byte-for-byte unchanged.
