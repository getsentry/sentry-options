from __future__ import annotations

import json
import os

import pytest
from sentry_options import init


@pytest.fixture(scope='session', autouse=True)
def init_options(tmp_path_factory: pytest.TempPathFactory) -> None:
    """
    Set up test data and initialize options once for the
    entire test session.
    """
    tmpdir = tmp_path_factory.mktemp('sentry_options')

    # Create schema including regular options and feature flag keys
    schema_dir = tmpdir / 'schemas' / 'sentry-options-testing'
    schema_dir.mkdir(parents=True)
    (schema_dir / 'schema.json').write_text(
        json.dumps(
            {
                'version': '1.0',
                'type': 'object',
                'properties': {
                    'str-opt': {
                        'type': 'string',
                        'default': 'default-value',
                        'description': 'A string option',
                    },
                    'int-opt': {
                        'type': 'integer',
                        'default': 42,
                        'description': 'An integer option',
                    },
                    'float-opt': {
                        'type': 'number',
                        'default': 3.14,
                        'description': 'A float option',
                    },
                    'bool-opt': {
                        'type': 'boolean',
                        'default': True,
                        'description': 'A boolean option',
                    },
                    'array-opt': {
                        'type': 'array',
                        'default': [1, 2, 3],
                        'items': {'type': 'integer'},
                        'description': 'A list of integers',
                    },
                    # Feature flag options (stored as serialized JSON strings)
                    'features.organizations:purple-site': {
                        'type': 'string',
                        'default': '',
                        'description': 'Feature flag for purple-site',
                    },
                    'features.organizations:disabled-flag': {
                        'type': 'string',
                        'default': '',
                        'description': 'A disabled feature flag',
                    },
                    'features.organizations:no-segments': {
                        'type': 'string',
                        'default': '',
                        'description': 'Feature flag with no segments',
                    },
                },
            },
        ),
    )

    # Feature flag values as serialized JSON strings
    purple_site = {
        'enabled': True,
        'segments': [
            {
                'name': 'internal',
                'rollout': 100,
                'conditions': [
                    {
                        'property': 'organization_slug',
                        'operator': {
                            'kind': 'in',
                            'value': ['sentry', 'sentry-test'],
                        },
                    },
                ],
            },
        ],
    }
    disabled_flag = {
        'enabled': False,
        'segments': [
            {
                'name': 'everyone',
                'rollout': 100,
                'conditions': [],
            },
        ],
    }
    no_segments = {
        'enabled': True,
        'segments': [],
    }

    values_dir = tmpdir / 'values' / 'sentry-options-testing'
    values_dir.mkdir(parents=True)
    values = {
        'options': {
            'str-opt': 'custom-value',
            'features.organizations:purple-site': json.dumps(purple_site),
            'features.organizations:disabled-flag': json.dumps(disabled_flag),
            'features.organizations:no-segments': json.dumps(no_segments),
        },
    }
    (values_dir / 'values.json').write_text(json.dumps(values))

    orig_env = os.environ.get('SENTRY_OPTIONS_DIR')
    os.environ['SENTRY_OPTIONS_DIR'] = str(tmpdir)

    init()

    yield

    if orig_env is None:
        os.environ.pop('SENTRY_OPTIONS_DIR', None)
    else:
        os.environ['SENTRY_OPTIONS_DIR'] = orig_env
