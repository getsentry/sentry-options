# Changelog
## 1.0.7

### New Features ✨

- (watcher) Fork-safe, PID-aware values watcher by @kenzoengineer in [#118](https://github.com/getsentry/sentry-options/pull/118)

## 1.0.6

### New Features ✨

- (schema) Support additionalProperties map schemas by @jan-auer in [#114](https://github.com/getsentry/sentry-options/pull/114)

### Bug Fixes 🐛

- (validation) Strip unknown keys from values to handle deployment race condition by @hubertdeng123 in [#117](https://github.com/getsentry/sentry-options/pull/117)

### Internal Changes 🔧

- (deps) Trim sentry SDK to only required features by @lcian in [#112](https://github.com/getsentry/sentry-options/pull/112)

## 1.0.5

### New Features ✨

- (clients) Add init_with_schemas() for compile-time embedded schemas by @hubertdeng123 in [#113](https://github.com/getsentry/sentry-options/pull/113)

## 1.0.4

### Bug Fixes 🐛

- (clients) Remove all panics from Rust client public API by @hubertdeng123 in [#111](https://github.com/getsentry/sentry-options/pull/111)

### Internal Changes 🔧

- Pin GitHub Actions to full-length commit SHAs by @joshuarli in [#110](https://github.com/getsentry/sentry-options/pull/110)

## 1.0.3

### New Features ✨

- (cli) Adds retry logic to schema retriever by @kenzoengineer in [#109](https://github.com/getsentry/sentry-options/pull/109)
- (docs) Update onboard/add-option skill to include rust examples by @hubertdeng123 in [#80](https://github.com/getsentry/sentry-options/pull/80)

### Other

- fix(feature) Expand readme and update defaults by @markstory in [#106](https://github.com/getsentry/sentry-options/pull/106)

## 1.0.2

### New Features ✨

- (clients) Include stub generation script and pre-commit hook by @kenzoengineer in [#104](https://github.com/getsentry/sentry-options/pull/104)
- (init) Allow options to be initialized more than once by @hubertdeng123 in [#107](https://github.com/getsentry/sentry-options/pull/107)
- Add feature flag checking for rust and python by @markstory in [#92](https://github.com/getsentry/sentry-options/pull/92)
- Add feature flag support to option schema by @markstory in [#91](https://github.com/getsentry/sentry-options/pull/91)

### Bug Fixes 🐛

- (stubs) Include \__all__ to explicitly re-export everything by @kenzoengineer in [#108](https://github.com/getsentry/sentry-options/pull/108)

## 1.0.1

### Bug Fixes 🐛

- (git) Assume non-matching pathspec is an empty directory by @kenzoengineer in [#105](https://github.com/getsentry/sentry-options/pull/105)

## 1.0.0

### New Features ✨

- Add type stubs for pythong client library by @hubertdeng123 in [#102](https://github.com/getsentry/sentry-options/pull/102)

### Bug Fixes 🐛

- (ci) Build for python 3.11 manylinux by @hubertdeng123 in [#103](https://github.com/getsentry/sentry-options/pull/103)
- (python) Correct abi3 wheel tagging to cp311 by @hubertdeng123 in [#101](https://github.com/getsentry/sentry-options/pull/101)

### Documentation 📚

- Add module-level docstring to Python client by @hubertdeng123 in [#100](https://github.com/getsentry/sentry-options/pull/100)

### Internal Changes 🔧

- (ci) Delete old composite action by @kenzoengineer in [#97](https://github.com/getsentry/sentry-options/pull/97)
- (docs) Update docs to match 0.0.15 features by @kenzoengineer in [#98](https://github.com/getsentry/sentry-options/pull/98)
- (release) Also bump reusable workflow version by @kenzoengineer in [#96](https://github.com/getsentry/sentry-options/pull/96)

## 0.0.15

### New Features ✨

- (ci) Migrate `validate-schema` composite action to become reusable workflow and include deletion check by @kenzoengineer in [#78](https://github.com/getsentry/sentry-options/pull/78)
- (core) Adds object support by @kenzoengineer in [#93](https://github.com/getsentry/sentry-options/pull/93)
- (testing) Option Overrides for testing by @hubertdeng123 in [#50](https://github.com/getsentry/sentry-options/pull/50)
- Add isset() by @markstory in [#85](https://github.com/getsentry/sentry-options/pull/85)

### Bug Fixes 🐛

#### Ci

- Use local checkout for deletion validation by @kenzoengineer in [#89](https://github.com/getsentry/sentry-options/pull/89)
- Remove reusable workflow call to fix CI by @kenzoengineer in [#88](https://github.com/getsentry/sentry-options/pull/88)

### Documentation 📚

- Use envoy injector to mount configmaps by @hubertdeng123 in [#90](https://github.com/getsentry/sentry-options/pull/90)

## 0.0.14

### New Features ✨

#### Cli

- Output deletions with flag by @kenzoengineer in [#79](https://github.com/getsentry/sentry-options/pull/79)
- Add option usage check by @kenzoengineer in [#77](https://github.com/getsentry/sentry-options/pull/77)

#### Other

- (docs) Update claude skills and docs by @hubertdeng123 in [#70](https://github.com/getsentry/sentry-options/pull/70)
- (release) Add uv setup step in release by @kenzoengineer in [#83](https://github.com/getsentry/sentry-options/pull/83)
- (validation) Adds supports for lists of primitives by @kenzoengineer in [#69](https://github.com/getsentry/sentry-options/pull/69)

### Bug Fixes 🐛

- (cli) Fix merge conflict issue by @kenzoengineer in [#82](https://github.com/getsentry/sentry-options/pull/82)

### Internal Changes 🔧

- (release) Regenerate the uv lock on release + update composite action to use musl by @kenzoengineer in [#76](https://github.com/getsentry/sentry-options/pull/76)
- Remove old python code by @hubertdeng123 in [#81](https://github.com/getsentry/sentry-options/pull/81)

## 0.0.13

### Bug Fixes 🐛

- (ci) Musl binary but manylinux python by @kenzoengineer in [#75](https://github.com/getsentry/sentry-options/pull/75)

## 0.0.12

### New Features ✨

- (deployment) Remove target from configmap filename by @kenzoengineer in [#74](https://github.com/getsentry/sentry-options/pull/74)
- (docs) Populate README.md by @kenzoengineer in [#64](https://github.com/getsentry/sentry-options/pull/64)
- (skills) Add onboard/add-option skills by @hubertdeng123 in [#68](https://github.com/getsentry/sentry-options/pull/68)
- (watcher) Allow suppression of missing dir errors with env var by @kenzoengineer in [#73](https://github.com/getsentry/sentry-options/pull/73)

## 0.0.11

### New Features ✨

- (release) Use manylinux by @hubertdeng123 in [#67](https://github.com/getsentry/sentry-options/pull/67)

## 0.0.10

### New Features ✨

- (validation) Improve value validation error message by @kenzoengineer in [#66](https://github.com/getsentry/sentry-options/pull/66)

### Internal Changes 🔧

- (ci) Bump validate-schema version to 0.0.9 and add auto bumping by @kenzoengineer in [#63](https://github.com/getsentry/sentry-options/pull/63)
- Update integration guide by @hubertdeng123 in [#58](https://github.com/getsentry/sentry-options/pull/58)

### Other

- demo: New string option by @kenzoengineer in [#65](https://github.com/getsentry/sentry-options/pull/65)

## 0.0.9

### New Features ✨

- (ci) Update composite action to selectively build from source by @kenzoengineer in [#60](https://github.com/getsentry/sentry-options/pull/60)
- (k8) Fail if generated config map is larger than 1 MiB by @kenzoengineer in [#61](https://github.com/getsentry/sentry-options/pull/61)

### Bug Fixes 🐛

- (cli) Correctly ignore non-yaml files by @kenzoengineer in [#62](https://github.com/getsentry/sentry-options/pull/62)
- (tests) Do not report spans for tests by @hubertdeng123 in [#57](https://github.com/getsentry/sentry-options/pull/57)

### Internal Changes 🔧

- (cli) Make schema evolution error more user friendly by @kenzoengineer in [#59](https://github.com/getsentry/sentry-options/pull/59)

## 0.0.8

### New Features ✨

- (examples) Test adding option and migrate example schemas by @hubertdeng123 in [#55](https://github.com/getsentry/sentry-options/pull/55)
- (sentry) Add sentry by @hubertdeng123 in [#48](https://github.com/getsentry/sentry-options/pull/48)

### Bug Fixes 🐛

- (compilation) Add openssl by @hubertdeng123 in [#56](https://github.com/getsentry/sentry-options/pull/56)

## 0.0.7

### New Features ✨

- (cli) Statically compile binary by @hubertdeng123 in [#53](https://github.com/getsentry/sentry-options/pull/53)
- (craft) Update craft to use musl by @hubertdeng123 in [#54](https://github.com/getsentry/sentry-options/pull/54)
- (docs) Add integration guide by @hubertdeng123 in [#49](https://github.com/getsentry/sentry-options/pull/49)

### Bug Fixes 🐛

- (library) Client library loads json that is compatible with cli output by @hubertdeng123 in [#46](https://github.com/getsentry/sentry-options/pull/46)

### Internal Changes 🔧

- (sandbox) Some instructions for testing in sandbox by @hubertdeng123 in [#47](https://github.com/getsentry/sentry-options/pull/47)

## 0.0.6

### New Features ✨

#### Ci

- Fix mac builds by @hubertdeng123 in [#45](https://github.com/getsentry/sentry-options/pull/45)
- Create composite action for schema validation by @kenzoengineer in [#42](https://github.com/getsentry/sentry-options/pull/42)

#### Other

- (cli) Add configmap output by @hubertdeng123 in [#43](https://github.com/getsentry/sentry-options/pull/43)
- (release) Upload CLI artifacts to github by @kenzoengineer in [#44](https://github.com/getsentry/sentry-options/pull/44)

## 0.0.5

### New Features ✨

#### Cli

- Add namespace name validation by @hubertdeng123 in [#41](https://github.com/getsentry/sentry-options/pull/41)
- Adds schema change validation CLI command by @kenzoengineer in [#37](https://github.com/getsentry/sentry-options/pull/37)
- Flatten schema fetching by @hubertdeng123 in [#38](https://github.com/getsentry/sentry-options/pull/38)
- Implement fetch-schemas command by @hubertdeng123 in [#36](https://github.com/getsentry/sentry-options/pull/36)

#### Other

- (ci) Validate schema changes between shas by @kenzoengineer in [#34](https://github.com/getsentry/sentry-options/pull/34)
- (config) Add logic for parsing repo config by @hubertdeng123 in [#35](https://github.com/getsentry/sentry-options/pull/35)
- (default) Add local default path sentry-options/ by @hubertdeng123 in [#39](https://github.com/getsentry/sentry-options/pull/39)

### Build / dependencies / internal 🔧

- (validation) Move path resolution logic to shared library by @kenzoengineer in [#40](https://github.com/getsentry/sentry-options/pull/40)
- Upgrade to action-setup-venv 3.2.0 and pin python to 3.14.2 by @joshuarli in [#33](https://github.com/getsentry/sentry-options/pull/33)

## 0.0.4

### New Features ✨

#### Release

- feat(release): Combine artifacts into one artifact for release by @hubertdeng123 in [#31](https://github.com/getsentry/sentry-options/pull/31)
- feat(release): Upload sdist and platform specific python wheels by @hubertdeng123 in [#30](https://github.com/getsentry/sentry-options/pull/30)

### Bug Fixes 🐛

- fix(release): Filters out sdist tar.gz for internal pypi by @kenzoengineer in [#32](https://github.com/getsentry/sentry-options/pull/32)

## 0.0.2

### New Features ✨

- feat(build): Build for python 3.11 by @hubertdeng123 in [#29](https://github.com/getsentry/sentry-options/pull/29)

- feat(license): Add license info to rust crates by @hubertdeng123 in [#28](https://github.com/getsentry/sentry-options/pull/28)

### Other

- fix newline by @hubertdeng123 in [968d9dc6](https://github.com/getsentry/sentry-options/commit/968d9dc6c6f5d5e9a6631c952354d946b878914c)
- release: 0.0.1 by @getsentry-bot in [701a1ccc](https://github.com/getsentry/sentry-options/commit/701a1ccc1aa33d6ce6aa8330aef3149aa5b76fd2)
- newline and content by @kenzoengineer in [#27](https://github.com/getsentry/sentry-options/pull/27)
- changelog by @kenzoengineer in [#26](https://github.com/getsentry/sentry-options/pull/26)

## 0.0.1

### New Features ✨

- feat(license): Add license info to rust crates by @hubertdeng123 in [#28](https://github.com/getsentry/sentry-options/pull/28)

### Other

- newline and content by @kenzoengineer in [#27](https://github.com/getsentry/sentry-options/pull/27)
- changelog by @kenzoengineer in [#26](https://github.com/getsentry/sentry-options/pull/26)
