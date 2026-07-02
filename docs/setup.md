# Setting up sentry-options in a new repo

## Preamble

Setting up `sentry-options` from scratch will require a couple PRs in three repos.
Once set up, refer to [Working with options](./options.md) for anything related to *using* the library (adding/removing options, testing, setting values, etc.).

```text
Phase 1: Your repo (aka "Service Repo") (e.g. `seer`, `snuba`)
    - schema, CI, precommit, dockerfile, library
Phase 2: sentry-options-automator
    - repos.json, values files
Phase 3: ops
    - envoy injector
```

## Phase 1 - Service Repo

> These can all be a part of the same PR

### 1. Create the schema

Create a new file called `sentry-options/schemas/{namespace}/schema.json`. `{namespace}` must either be your repo name, or prefixed by `{repo}-`. e.g. for the `seer` repo:

```text
seer          - ok!     namespace is the repo name
seer-testing  - ok!     namespace is prefixed by {repo}-
seertesting   - nope    missing hyphen in prefix
seer.testing  - nope    missing hyphen in prefix
testing       - nope    is not namespace nor prefixed by namespace
```

In this new file, create the following schema:

```json
{
  "version": "1.0",
  "type": "object",
  "properties": {}
}
```

`version` and `type` are required keys in all schemas. `properties` is where you will start putting your new options. You can leave this empty for now, or add some initial options. See [the user guide](./options.md#adding-an-option) for details on what option and feature schemas look like.

### 2. Add workflows

It is important to perform validation in your own repo to make sure sentry-options doesn't break your service. This workflow will ensure there are no syntax errors and no [evolution rules](./architecture.md#schema-evolution--validation) are violated (changing the type of an option, changing the default of an option, etc.)

In any workflow that runs in PRs and your main branch, add this reusable workflow:

```yaml
jobs:
  # Very important to use the same options cli version for validation.
  options-cli-version:
    runs-on: ubuntu-latest
    outputs:
      version: ${{ steps.version.outputs.version }}
    steps:
      - uses: actions/checkout@v6
        with:
          sparse-checkout: uv.lock # or Cargo.lock; adapt the yq call as necessary
          sparse-checkout-cone-mode: false
      - name: Parse sentry-options version from lockfile
        id: version
        run: |
          echo -n "version=" >> "$GITHUB_OUTPUT"
          yq -p toml '.package[] | select(.name == "sentry-options") | .version' uv.lock >> "$GITHUB_OUTPUT"

  validate:
    needs: cli-version
    uses: getsentry/sentry-options/.github/workflows/validate-schema.yml@0b115be89b102d76beff8106bd4054365954282e
    secrets:
      SENTRY_INTERNAL_APP_PRIVATE_KEY: ${{ secrets.SENTRY_INTERNAL_APP_PRIVATE_KEY }}
    with:
      schemas-path: sentry-options/schemas
      cli-version: ${{ needs.cli-version.outputs.version }}
```

Additionally, if you are using the **Python** library, we recommend adding our precommit hook to autogenerate type stubs. This means you don't need to `cast(...)` options in your code to pass `mypy` or `ty` typechecking.

In `.pre-commit-config.yaml`

```yaml
  # Generates sentry-options type stubs
  - repo: https://github.com/getsentry/sentry-options
    rev: 0b115be89b102d76beff8106bd4054365954282e
    hooks:
      - id: sentry-options-gen-stubs
```

Again, check the repo for the latest SHAs (pinning actions at SHAs is better practice than pinning at versions, like @1.1.0)

### 3. Add the client library

In Python, this would be in `pyproject.toml` or similar:

```toml
dependencies = ["sentry-options>=1.1.1"]
```

In Rust, this would be in `Cargo.toml`:

```toml
[dependencies]
sentry-options = "1.1.1"
```

Check the [releases page](https://github.com/getsentry/sentry-options/releases) for the latest version.

### 4. Initialize `sentry-options`

Call `init()` once, as early in startup as possible (e.g. right where you initialize Sentry).

#### Python

```python
from sentry_options import init

init()  # optional: init(on_propagation=Callback)
```

`on_propagation` is a callback fired whenever values reload, useful for collecting metrics.

#### Rust

Rust doesn't support optional params, so the variants are separate functions:

| Function                             | Use when                                                                                                                                      |
| ------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `init()`                             | Load schemas from the fallback path (`SENTRY_OPTIONS_DIR` → `/etc/sentry-options` → `./sentry-options/`). `Err`s if the directory is missing. |
| `init_with_schemas(&[(ns, json)])`   | Embed schemas in the binary via `include_str!` so there's no disk dependency for schemas. Values are still read from disk and hot-reloaded.   |
| `init_with_propagation_callback(cb)` | Like `init()`, plus the same on_propagation callback                                                                                          |

More details about the `init()` and `propagation_callback` signature and usage can be found in the function documentation and [architecture doc](./architecture.md).

### 5. Copy schemas folder and set `SENTRY_OPTIONS_DIR`

The recommended path for sentry-options in production is `/etc/sentry-options`. This is where schemas and values will be mounted. Values are placed there automatically in phase 3 but we need to manually copy over the schemas there from our local path.

In your `Dockerfile` or equivalent:

```dockerfile
COPY sentry-options/schemas /etc/sentry-options/schemas
ENV SENTRY_OPTIONS_DIR=/etc/sentry-options
```

The `COPY` step can be omitted if using Rust `init_with_schemas()` but explicitly setting `SENTRY_OPTIONS_DIR` is still recommended.

### Phase 1 Summary

At this point, you have done all the setup to use `sentry-options` in your repo. If you'd like, you can follow the [usage guide](./options.md) to add an option to the schema and start retrieving its default value in code.

## Phase 2: `sentry-options-automator`

### 1. Register the repo

For validation to succeed in `sentry-options-automator` it must first know your repo is using sentry options. This is done by adding your repo to `./repos.json`. For example, for `seer`:

```json
{
  "repos": {
    "seer": {
      "url": "https://github.com/getsentry/seer",
      "path": "sentry-options/"
    }
  }
}

```

### 2. Add the values files and directories

Option values live in `option-values/{namespace}/{target}/values.yaml`. Start by creating a `values.yaml` for the mandatory `default` target.

In `option-values/{namespace}/default/values.yaml`:

```yaml
options: {}
```

Every other `target` is synonymous with "region". **You must add a folder and corresponding `values.yaml` for every region you want to deploy to. Just having `/default/` will not deploy to any region**. However, values will inherit from `default` first, so if you want the same value in every region, just change `/default/` and leave all other targets as `options: {}`.

Follow the directions in the [user guide](./options.md#setting-an-option-value) for more information on how to actually set the value of an option.

### Phase 2 Summary

You have now made your repo known to `sentry-options-automator` and scaffolded the values files. You can now actually set the value of an option, commit it, and the GoCD pipeline will deploy your config maps to the relevant regions. However, your pods won't pick up and mount the configmaps by default. Thus we move on to the last step.

## Phase 3: `ops`

With help from SRE, we've made it really easy to automatically mount configmaps into your pods

### 1. Add injector annotations

In your Kubernetes config file (usually a `deployment.yaml`), add the following annotations (`seer` is used as an example here):

```yaml
spec:
  template:
    metadata:
      annotations:
        options.sentry.io/inject: 'true'
        options.sentry.io/namespace: seer
```

Any pod that uses this workload spec in the `default` Kubernetes namespace will automatically mount the configmaps. The actual mechanism is explained in more detail in the [architecture doc](./architecture.md)

For multiple namespaces, use a comma-separated list:

```yaml
spec:
  template:
    metadata:
      annotations:
        options.sentry.io/inject: 'true'
        options.sentry.io/namespace: seer,seer-code-review
```

Don't forget to redeploy your infrastructure for these changes to take effect.

### Phase 3 Summary

With the injector annotations, your pods will now automatically pick up any new changes `sentry-options-automator` makes to your ConfigMaps. You can now move on to the [user guide](./options.md) for information on how to add/remove options, consume options, change option values, and test with options.
