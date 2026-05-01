# Options inventory

To run the options inventory script, run:

```bash
python ~/code/sentry-options/scripts/migration/option_inventory.py \
    --sentry <PATH_TO_SENTRY> \
    --getsentry <PATH_TO_GETSENTRY> \
    > <OUTPUT>
```

e.g.

```bash
python ~/code/sentry-options/scripts/migration/option_inventory.py \
    --sentry ~/code/sentry \
    --getsentry ~/code/getsentry \
    > ~/code/sentry-options/scripts/migration/inventory.csv
```

This will search both repos, look for options registrations, then print:

```text
sentry SHA: 28e76ad41a3807b2c79b96fa7b69844b998dbd35
getsentry SHA: a350b191b901cee39bff25d1e025624f52ad1e50
total options: 818

registrations per file:
   669  src/sentry/options/defaults.py
   117  getsentry/models/__init__.py
    28  src/sentry/hybridcloud/options.py
     2  src/sentry/auth/providers/fly/apps.py
     2  src/sentry/auth/providers/google/apps.py
```

This is self explanatory. The script should take a second or two to run. I didn't commit `inventory.csv` because it has data about getsentry options, a private repo.
