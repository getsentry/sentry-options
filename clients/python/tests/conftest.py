"""Shared pytest fixtures for sentry-options tests."""
from __future__ import annotations

import pytest
from sentry_options import init


@pytest.fixture(scope='session', autouse=True)
def _init_options() -> None:
    """Initialize options once for the test session."""
    init()
=======
from __future__ import annotations

import json
import os
from collections.abc import Generator

import pytest
from sentry_options import init

NAMESPACE = 'sentry-options-testing'


@pytest.fixture(scope='session', autouse=True)
def init_options(tmp_path_factory: pytest.TempPathFactory) -> Generator[None]:
    """
    Set up test schemas and initialize global options
    once for the whole test session.
    """
    tmpdir = tmp_path_factory.mktemp('sentry_options')

    # Create schema with regular options and feature flag entries
    schema_dir = tmpdir / 'schemas' / NAMESPACE
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
                    # Feature flag entries
                    'feature.organizations:enabled-feature': {
                        '$ref': '#/definitions/Feature',
                    },
                    'feature.organizations:disabled-feature': {
                        '$ref': '#/definitions/Feature',
                    },
                    'feature.organizations:rollout-zero': {
                        '$ref': '#/definitions/Feature',
                    },
                },
            },
        ),
    )

    # Create values with regular option overrides and feature flag definitions
    values_dir = tmpdir / 'values' / NAMESPACE
    values_dir.mkdir(parents=True)
    (values_dir / 'values.json').write_text(
        json.dumps(
            {
                'options': {
                    'str-opt': 'custom-value',
                    'feature.organizations:enabled-feature': {
                        'name': 'enabled-feature',
                        'enabled': True,
                        'owner': {'team': 'test-team'},
                        'created_at': '2024-01-01',
                        'segments': [
                            {
                                'name': 'org-segment',
                                'rollout': 100,
                                'conditions': [
                                    {
                                        'property': 'organization_id',
                                        'operator': 'in',
                                        'value': [123, 456],
                                    },
                                ],
                            },
                        ],
                    },
                    'feature.organizations:disabled-feature': {
                        'name': 'disabled-feature',
                        'enabled': False,
                        'owner': {'team': 'test-team'},
                        'created_at': '2024-01-01',
                        'segments': [
                            {
                                'name': 'org-segment',
                                'rollout': 100,
                                'conditions': [
                                    {
                                        'property': 'organization_id',
                                        'operator': 'in',
                                        'value': [123],
                                    },
                                ],
                            },
                        ],
                    },
                    'feature.organizations:rollout-zero': {
                        'name': 'rollout-zero',
                        'enabled': True,
                        'owner': {'team': 'test-team'},
                        'created_at': '2024-01-01',
                        'segments': [
                            {
                                'name': 'org-segment',
                                'rollout': 0,
                                'conditions': [
                                    {
                                        'property': 'organization_id',
                                        'operator': 'in',
                                        'value': [123],
                                    },
                                ],
                            },
                        ],
                    },
                },
            },
        ),
    )

    orig_env = os.environ.get('SENTRY_OPTIONS_DIR')
    os.environ['SENTRY_OPTIONS_DIR'] = str(tmpdir)

    init()

    yield

    if orig_env is None:
        os.environ.pop('SENTRY_OPTIONS_DIR', None)
    else:
        os.environ['SENTRY_OPTIONS_DIR'] = orig_env
