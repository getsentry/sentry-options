from __future__ import annotations

import pytest
from sentry_options import Assignment
from sentry_options import experiment_property
from sentry_options import ExperimentChecker
from sentry_options import ExperimentError
from sentry_options import experiments
from sentry_options import SchemaError
from sentry_options import UnknownNamespaceError
from sentry_options.testing import experiment
from sentry_options.testing import override_options

NAMESPACE = 'sentry-options-testing'


def test_experiments_returns_checker():
    checker = experiments(NAMESPACE)
    assert isinstance(checker, ExperimentChecker)
    assert NAMESPACE in repr(checker)


def test_experiment_property():
    assert experiment_property() == {'$ref': '#/definitions/Experiment'}


@pytest.mark.parametrize(
    ('org', 'slot', 'status', 'arm', 'excluded_by'),
    [
        (1, 6, 'assigned', 'control', None),
        (5, 3, 'assigned', 'treatment', None),
        (16, 58, 'excluded', None, 'checkout-copy'),
        (4, 74, 'holdout', None, None),
    ],
)
def test_pinned_assignments_match_rust(org, slot, status, arm, excluded_by):
    a = experiments(NAMESPACE).assign('checkout-color', {'organization_id': org})
    assert isinstance(a, Assignment)
    assert (a.slot, a.status, a.arm, a.excluded_by) == (slot, status, arm, excluded_by)
    assert a.namespace == NAMESPACE
    assert a.experiment == 'checkout-color'
    assert a.layer == 'checkout'
    assert a.unit == ['organization_id']
    assert a.subject == str(org)
    assert a.is_assigned() is (status == 'assigned')
    assert a.in_arm('treatment') is (arm == 'treatment')


def test_config_is_returned_for_the_assigned_arm():
    a = experiments(NAMESPACE).assign('checkout-color', {'organization_id': 5})
    assert a.config == {'color': 'green'}
    assert experiments(NAMESPACE).assign('checkout-color', {'organization_id': 1}).config is None


def test_sibling_experiment_owns_the_excluded_slot():
    a = experiments(NAMESPACE).assign('checkout-copy', {'organization_id': 16})
    assert (a.status, a.arm) == ('assigned', 'short')
    assert experiments(NAMESPACE).assign('checkout-copy', {'organization_id': 37}).arm == 'long'


def test_disabled_experiment():
    a = experiments(NAMESPACE).assign('paused-experiment', {'run_id': 1})
    assert (a.status, a.slot, a.arm) == ('disabled', 13, None)


def test_unconfigured_experiment_is_unassigned():
    a = experiments(NAMESPACE).assign('unconfigured-experiment', {'organization_id': 1})
    assert a.status == 'unassigned'
    assert a.reason == 'experiment is not configured'
    assert a.layer is None and a.slot is None and a.subject is None


def test_unknown_experiment_is_unassigned():
    a = experiments(NAMESPACE).assign('does-not-exist', {'organization_id': 1})
    assert a.status == 'unassigned'


def test_missing_unit_field():
    a = experiments(NAMESPACE).assign('checkout-color', {})
    assert a.status == 'unassigned'
    assert 'organization_id' in a.reason
    with pytest.raises(ExperimentError, match='organization_id'):
        experiments(NAMESPACE).try_assign('checkout-color', {'organization_id': None})


def test_try_assign_unknown_namespace_raises():
    with pytest.raises(UnknownNamespaceError):
        experiments('nope').try_assign('checkout-color', {'organization_id': 1})
    assert experiments('nope').assign('checkout-color', {'organization_id': 1}).status == 'unassigned'


def test_to_dict_is_the_exposure_record():
    a = experiments(NAMESPACE).assign('checkout-color', {'organization_id': 5})
    assert a.to_dict() == {
        'namespace': NAMESPACE,
        'experiment': 'checkout-color',
        'layer': 'checkout',
        'unit': 'organization_id',
        'subject': '5',
        'slot': 3,
        'status': 'assigned',
        'arm': 'treatment',
        'excluded_by': None,
        'reason': None,
    }


def test_repr_mentions_experiment_status_and_arm():
    a = experiments(NAMESPACE).assign('checkout-color', {'organization_id': 5})
    assert 'checkout-color' in repr(a)
    assert 'treatment' in repr(a)


def test_layer_lists_members_in_slot_order():
    members = experiments(NAMESPACE).layer('checkout')
    assert members == [
        {'experiment': 'checkout-color', 'start': 0, 'size': 40, 'enabled': True},
        {'experiment': 'checkout-copy', 'start': 40, 'size': 30, 'enabled': True},
    ]
    assert experiments(NAMESPACE).layer('unknown') == []


def test_string_unit_values_are_used_verbatim():
    a = experiments(NAMESPACE).assign('checkout-color', {'organization_id': '1'})
    assert a.slot == 6


def test_override_replaces_experiment_for_the_test():
    with override_options(
        NAMESPACE,
        {
            'experiment.checkout-color': experiment(
                layer='checkout',
                unit=['organization_id'],
                arms={'control': 0, 'treatment': 100},
                start=70,
                size=30,
            ),
        },
    ):
        a = experiments(NAMESPACE).assign('checkout-color', {'organization_id': 4})
        assert (a.status, a.arm) == ('assigned', 'treatment')
    assert experiments(NAMESPACE).assign('checkout-color', {'organization_id': 4}).status == 'holdout'


def test_override_validates_experiment_shape():
    with pytest.raises(Exception):
        with override_options(NAMESPACE, {'experiment.checkout-color': {'layer': 'x'}}):
            pass


def test_override_with_overlapping_allocation_raises():
    with pytest.raises(SchemaError, match='overlap'):
        with override_options(
            NAMESPACE,
            {
                'experiment.checkout-color': experiment(
                    layer='checkout',
                    unit=['organization_id'],
                    arms={'control': 50, 'treatment': 50},
                    start=30,
                    size=30,
                ),
            },
        ):
            pass


def test_assign_with_unconvertible_context_is_unassigned():
    a = experiments(NAMESPACE).assign('checkout-color', {'organization_id': float('nan')})
    assert a.status == 'unassigned'
    assert isinstance(a.reason, str) and a.reason
    with pytest.raises(ValueError):
        experiments(NAMESPACE).try_assign('checkout-color', {'organization_id': float('nan')})


def test_documented_reference_hash_matches_extension():
    import hashlib

    def bucket(components: list[str], modulus: int) -> int:
        h = hashlib.sha1()
        for c in components:
            b = c.encode()
            h.update(len(b).to_bytes(8, 'big'))
            h.update(b)
        return int.from_bytes(h.digest()[:8], 'big') % modulus

    slot = bucket(['layer', NAMESPACE, 'checkout', '1'], 100)
    assert slot == 6
    assert slot == experiments(NAMESPACE).assign('checkout-color', {'organization_id': 1}).slot

    assert bucket(['experiment', NAMESPACE, 'checkout-color', '5'], 100) >= 50
    assert experiments(NAMESPACE).assign('checkout-color', {'organization_id': 5}).arm == 'treatment'


def test_testing_experiment_builder_defaults():
    value = experiment(layer='l', unit=['run_id'], arms={'control': 50, 'treatment': 50})
    assert value == {
        'owner': {'team': 'testing'},
        'layer': 'l',
        'unit': ['run_id'],
        'allocation': {'start': 0, 'size': 100},
        'enabled': True,
        'arms': [
            {'name': 'control', 'weight': 50},
            {'name': 'treatment', 'weight': 50},
        ],
    }
