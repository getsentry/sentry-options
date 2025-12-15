from __future__ import annotations

from sentry_options_old.api import option_group

option_group(1)  # type: ignore[arg-type]

invalid = option_group('definitely-not-a-group')  # type: ignore[arg-type]
# invalid option group does not crash plugin
invalid.get('anything')

sentry = option_group('sentry')
sentry.get(1)  # type: ignore[arg-type]
sentry.get('definitely-not-a-key')  # type: ignore[arg-type]
