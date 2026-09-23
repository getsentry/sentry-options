How options work in sentry (`~/dev/sentry`) and getsentry (`~/dev/getsentry`)

## Configuration Layers: Self-Hosted vs SaaS

There are two distinct deployment paths, and they configure Sentry very differently.

### Self-Hosted (open-source sentry repo)

Configuration lives in two separate operator-managed files, usually under `~/.sentry/`:

**`~/.sentry/sentry.conf.py`** — Python file, exec'd by `importer.py` directly onto the Django
settings module. Every uppercase name becomes a Django setting. Loads on top of
`src/sentry/conf/server.py`. This is where operators write:

```python
DATABASES = {...}
CACHES = {...}
```

**`~/.sentry/config.yml`** — YAML file, parsed by `bootstrap_options()`. Keys use dot notation
and map to Sentry options:

```yaml
system.secret-key: "..."
mail.host: smtp.example.com
system.support-email: support@example.com
```

These are not two formats of the same thing. `config.yml` feeds `settings.SENTRY_OPTIONS`
(the options system). `sentry.conf.py` sets Django settings directly.

**`src/sentry/conf/server.py`** — in-repo base defaults for all Django settings. Never edited
by operators; edited by engineers to add or change settings.

**The `options_mapper` bridge** (`initializer.py`): a small dict that promotes a subset of
YAML options into Django settings before `django.setup()`. Exists because Django internals and
third-party libs read settings like `EMAIL_HOST` directly — they can't go through
`options.get()`. The mapper reads `SENTRY_OPTIONS` and calls `setattr(settings, ...)`.

### SaaS (getsentry repo)

GetSentry does not use `config.yml` or `sentry.conf.py` at all. It is a pure Python settings
hierarchy that starts by importing everything from Sentry and then overriding:

```
DJANGO_SETTINGS_MODULE=getsentry.settings  (always)
          │
          └─► getsentry/settings.py
                  │
                  ├─ from getsentry.conf.settings.defaults import *
                  │       └─ from sentry.conf.server import *   ← Sentry base defaults
                  │          (then GetSentry extensions, SENTRY_OPTIONS assignments, etc.)
                  │
                  └─ DJANGO_CONF env var selects environment overlay:
                       dev         → getsentry/conf/settings/dev.py
                       test        → getsentry/conf/settings/test.py
                       cellsilo    → getsentry/conf/settings/cellsilo.py   (region silo prod)
                       controlsilo → getsentry/conf/settings/controlsilo.py
```

`DJANGO_CONF` (or `GETSENTRY_DJANGO_CONF`) is a GetSentry-custom env var that `getsentry/settings.py`
reads to dynamically `__import__` the appropriate module and merge its globals. It is not a
Django concept.

In getsentry, options are set directly in Python rather than via YAML:

```python
# getsentry/conf/settings/defaults.py
SENTRY_OPTIONS["system.support-email"] = "support@sentry.io"
SENTRY_OPTIONS["system.admin-email"] = "hello@sentry.io"
```

Because getsentry's `defaults.py` starts with `from sentry.conf.server import *`, the
`SENTRY_OPTIONS` dict already exists (defined in `server.py`) and is mutated in place.

## Architecture: Initialization Order

### Self-Hosted

```
importer.py
  └─► loads src/sentry/conf/server.py (base defaults)
  └─► exec's ~/.sentry/sentry.conf.py (operator Python overrides)

initialize_app(config_path)
        │
        ├─ 1. bootstrap_options(settings, ~/.sentry/config.yml)
        │       ├─ load_defaults()           ← registers options in defaults.py
        │       ├─ parse config.yml          ← raw YAML dict
        │       ├─ COMPAT: old Django settings → warn + copy to SENTRY_OPTIONS
        │       ├─ dump config.yml values    → settings.SENTRY_OPTIONS
        │       └─ options_mapper promotion  → e.g. "mail.from" → settings.SERVER_EMAIL
        │
        ├─ 2. configure_structlog()
        ├─ 3. django.setup()          ◄── SETTINGS FROZEN AFTER THIS POINT
        ├─ 4. validate_options()
        ├─ 5. bind_cache_to_option_store()
        ├─ 6. register_plugins() / initialize_receivers()
        └─ 7. configure_sdk()
```

### SaaS (GetSentry)

```
DJANGO_SETTINGS_MODULE=getsentry.settings
  └─► getsentry/conf/settings/defaults.py
        └─► from sentry.conf.server import *
        └─► SENTRY_OPTIONS["x.y"] = ...  (set directly in Python)
  └─► getsentry/conf/settings/{DJANGO_CONF}.py
        └─► additional overrides for environment

initialize_app()  ← same Sentry function, called via Django AppConfig
        │         ← no config.yml; SENTRY_OPTIONS already populated by settings import
        ├─ bootstrap_options(settings, config=None)
        │       ├─ load_defaults()
        │       └─ options_mapper promotion (reads from already-populated SENTRY_OPTIONS)
        ├─ django.setup()          ◄── SETTINGS FROZEN
        └─ ... (same as above)
```

**Django settings** (`settings.KEY`): immutable after `django.setup()`. Set by `sentry.conf.py`
(self-hosted) or by the getsentry Python hierarchy (SaaS). Base defaults always come from
`src/sentry/conf/server.py`.

**Sentry options** (`options.get("x.y")`): registered in `src/sentry/options/defaults.py`.
Read path at request time:

1. `FLAG_PRIORITIZE_DISK` set and key in `settings.SENTRY_OPTIONS`? → return immediately
2. Local in-memory cache (10s TTL, 60s grace period)
3. Redis shared cache
4. Database (`Option` / `ControlOption` model)
5. `settings.SENTRY_DEFAULT_OPTIONS` (registered defaults)
