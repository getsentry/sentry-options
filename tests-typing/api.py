from __future__ import annotations

from typing import assert_type
from typing import Literal

from sentry_options.api import option_group
from sentry_options.api import OptionGroup
from sentry_options.schema.testing import _TD

# option groups can be represented by their Literal name
assert_type(option_group('sentry'), OptionGroup[Literal['sentry']])
assert_type(option_group('testing'), OptionGroup[Literal['testing']])

# types are propagated from option definitions
assert_type(option_group('testing').get('example-option'), str)
assert_type(option_group('testing').get('list-option'), list[str])
assert_type(option_group('testing').get('dict-option'), dict[str, int])
assert_type(option_group('testing').get('typeddict-option'), _TD)
assert_type(option_group('testing').get('typeddict-option')['x'], int)
