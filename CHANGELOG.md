# Changelog
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
