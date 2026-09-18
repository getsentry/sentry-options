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
"experiment-layer.autofix": {
  "$ref": "#/definitions/ExperimentLayer"
}
```

or, to declare layers by pattern, `"patternProperties": { "^experiment-layer\\.": { "$ref": "#/definitions/ExperimentLayer" } }`. Like feature flags, layer keys take no `type` or `default`, and they are exempt from schema-evolution checks. A standalone experiment is a layer with one member; the layer name may simply repeat the experiment name.

## Setting an experiment value

In the automator's `option-values/{namespace}/{target}/values.yaml`, everything in a layer sits in one block, so you always see a layer's neighbours and their slots before adding to it:

```yaml
experiment-layer.autofix:
  description: Autofix experiments
  unit: [run_id]
  experiments:
    autofix-model:
      owner: { team: seer }
      description: Candidate vs default model
      allocation: { start: 0, size: 40 }
      arms:
        - { name: control, weight: 50 }
        - { name: treatment, weight: 50, config: { model: candidate } }
    autofix-prompt:
      owner: { team: seer }
      allocation: { start: 40 }
      arms:
        - { name: short, weight: 50 }
        - { name: long, weight: 50 }
```

| Field | Meaning |
| --- | --- |
| `unit` (layer) | Context fields that identify the subject. Lives on the layer; every experiment in it shares the unit. |
| `experiments` (layer) | The layer's experiments, keyed by name. At least one; each claims a disjoint allocation. |
| `allocation` | `start` is the first slot (0-99), `size` the number of slots, i.e. percent of the layer. `size` is any whole number from 1 to 100 and defaults to 20. `{start: 0, size: 40}` covers slots 0 through 39. |
| `arms` | 1 to 10 `{name, weight, config?}`. Weights are relative integers up to 1,000,000,000; `config` is free-form JSON your code reads. |
| `enabled` | Optional, default `true`. When `false`, subjects in the allocation are reported as `disabled` and get no arm. The allocation stays reserved. |
| `owner`, `description`, `created_at` | Same shape as on feature flags; only `owner.team` is required. |

The default allocation is 20 slots because that is what reaches significance on current Autofix traffic in about a week. Rough sample size per arm is `n ≈ 16 × p × (1 − p) / effect²`; at a 30% baseline a 5-point lift needs about 1,350 subjects per arm. In the week of Sep 9 2026 Autofix ran about 1,450 runs and 290 orgs a day, so a two-arm, run-unit experiment on 20% of a layer collects roughly 145 runs per arm per day and clears 1,350 in about nine days. Take the default and leave the rest of the layer free for the next experiment; raise `size` only when you need the traffic sooner and can spare the slots. Anything from 1 to 100 is allowed. Below about 10 slots an experiment on Autofix-scale traffic will take months to reach significance, so go that low only when the traffic is large enough that the sizing formula above says it works.

Validation rejects allocations that run past slot 99, allocations of zero slots, overlapping allocations in a layer, an experiment name declared in two layers, duplicate arm names, and an enabled experiment whose weights sum to 0 (a paused one may zero them). The overlap message names the two experiments, their slot ranges, and the layer's free slots.

### Sharing a layer

Because the whole layer is one block, adding a second experiment means editing the block and picking free slots the validator prints for you:

```yaml
experiment-layer.autofix:
  unit: [run_id]
  experiments:
    autofix-model:
      allocation: { start: 0, size: 40 }    # slots 0-39
    autofix-prompt:
      allocation: { start: 40, size: 30 }   # slots 40-69; 70-99 stay holdout
```

If the layer is full, wait for a running experiment to end or, if the two experiments are unrelated, give the new one its own layer. Shrinking a running experiment to make room keeps the remaining subjects in their arms but ends its phase.

To end an experiment, remove it from the layer's `experiments`; its slots become holdout. For long-lived units like organizations, the next experiment placed on those slots inherits a cohort that was just treated. Rename the layer (for example `experiment-layer.autofix-v2`) when that matters.

### Overriding a layer for one target

Values in `{target}/values.yaml` (for example `de` or a single tenant) replace the `default` value key by key, and a layer is one key. An override therefore replaces the whole layer for that target: nothing inside it is merged with the default. Repeat every experiment that should keep running there, not just the one you are changing.

```yaml
# option-values/seer/default/values.yaml
experiment-layer.autofix:
  unit: [run_id]
  experiments:
    autofix-model: { owner: { team: seer }, allocation: { start: 0, size: 40 }, arms: [...] }
    autofix-prompt: { owner: { team: seer }, allocation: { start: 40 }, arms: [...] }

# option-values/seer/de/values.yaml: pause autofix-model in de only
experiment-layer.autofix:
  unit: [run_id]
  experiments:
    autofix-model: { owner: { team: seer }, allocation: { start: 0, size: 40 }, arms: [...], enabled: false }
    autofix-prompt: { owner: { team: seer }, allocation: { start: 40 }, arms: [...] }   # omit this and de stops running it
```

This keeps a target's layer fully visible in one place, and the overridden layer is validated on its own, so overlaps are still caught. The merged values for each target are validated too, so an experiment name reused across layers is caught even when those layers sit in different files. A changed definition also gets its own `definition_revision`, so rows from that target are told apart in analysis.
