from __future__ import annotations

from datetime import datetime
from datetime import timezone

import pytest
from sentry_options import Assignment
from sentry_options import experiment_layer_property
from sentry_options import ExperimentError
from sentry_options import experiments
from sentry_options import SchemaError
from sentry_options import UnknownNamespaceError
from sentry_options.testing import experiment
from sentry_options.testing import experiment_layer
from sentry_options.testing import override_options

NAMESPACE = 'sentry-options-testing'


def test_experiment_layer_property():
    assert experiment_layer_property() == {'$ref': '#/definitions/ExperimentLayer'}


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
    assert a.subject == f'["{org}"]'
    assert a.is_assigned is (status == 'assigned')
    assert a.in_arm('treatment') is (arm == 'treatment')


def test_is_assigned_is_a_property_not_a_method():
    unconfigured = experiments(NAMESPACE).assign('unconfigured-experiment', {'organization_id': 1})
    assert unconfigured.status == 'unassigned'
    assert not unconfigured.is_assigned
    assigned = experiments(NAMESPACE).assign('checkout-color', {'organization_id': 1})
    assert assigned.is_assigned


def test_config_is_returned_for_the_assigned_arm():
    a = experiments(NAMESPACE).assign('checkout-color', {'organization_id': 5})
    assert a.config == {'color': 'green'}
    assert experiments(NAMESPACE).assign('checkout-color', {'organization_id': 1}).config is None


def test_sibling_experiment_owns_the_excluded_slot():
    a = experiments(NAMESPACE).assign('checkout-copy', {'organization_id': 16})
    assert (a.status, a.arm) == ('assigned', 'long')
    assert experiments(NAMESPACE).assign('checkout-copy', {'organization_id': 37}).arm == 'short'


def test_disabled_experiment():
    a = experiments(NAMESPACE).assign('paused-experiment', {'run_id': 1})
    assert (a.status, a.slot, a.arm) == ('disabled', 13, None)


def test_unconfigured_experiment_is_unassigned():
    a = experiments(NAMESPACE).assign('unconfigured-experiment', {'organization_id': 1})
    assert a.status == 'unassigned'
    assert a.reason == 'experiment is not configured'
    assert a.layer is None and a.slot is None and a.subject is None


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
        'unit': ['organization_id'],
        'subject': '["5"]',
        'slot': 3,
        'allocation_start': 0,
        'allocation_size': 40,
        'definition_revision': 'b6df71182b5566f6',
        'status': 'assigned',
        'arm': 'treatment',
        'excluded_by': None,
        'reason': None,
    }


def test_exposure_appends_version_service_and_recorded_at():
    a = experiments(NAMESPACE).assign('checkout-color', {'organization_id': 1})
    record = a.exposure('seer')
    recorded_at = record.pop('recorded_at')
    assert recorded_at.endswith('Z')
    assert datetime.fromisoformat(recorded_at).tzinfo == timezone.utc
    assert record == {
        'namespace': NAMESPACE,
        'experiment': 'checkout-color',
        'layer': 'checkout',
        'unit': ['organization_id'],
        'subject': '["1"]',
        'slot': 6,
        'allocation_start': 0,
        'allocation_size': 40,
        'definition_revision': 'b6df71182b5566f6',
        'status': 'assigned',
        'arm': 'control',
        'excluded_by': None,
        'reason': None,
        'record_version': 1,
        'service': 'seer',
    }


def test_exposure_without_timestamp_uses_now():
    record = experiments(NAMESPACE).assign('checkout-color', {'organization_id': 1}).exposure('seer')
    assert record['service'] == 'seer'
    assert record['record_version'] == 1
    parsed = datetime.fromisoformat(record['recorded_at'])
    assert abs((datetime.now(timezone.utc) - parsed).total_seconds()) < 5


@pytest.mark.parametrize(
    ('org', 'status', 'start', 'size'),
    [
        (1, 'assigned', 0, 40),
        (16, 'excluded', 0, 40),
        (4, 'holdout', 0, 40),
    ],
)
def test_allocation_is_recorded_for_configured_statuses(org, status, start, size):
    a = experiments(NAMESPACE).assign('checkout-color', {'organization_id': org})
    assert a.status == status
    assert (a.allocation_start, a.allocation_size) == (start, size)
    record = a.to_dict()
    assert record['allocation_start'] == start
    assert record['allocation_size'] == size


def test_disabled_experiment_carries_its_allocation():
    a = experiments(NAMESPACE).assign('paused-experiment', {'run_id': 1})
    assert a.status == 'disabled'
    assert (a.allocation_start, a.allocation_size) == (0, 100)
    assert a.to_dict()['allocation_start'] == 0
    assert a.to_dict()['allocation_size'] == 100


def test_unconfigured_experiment_has_no_allocation():
    a = experiments(NAMESPACE).assign('unconfigured-experiment', {'organization_id': 1})
    assert a.allocation_start is None
    assert a.allocation_size is None
    assert a.to_dict()['allocation_start'] is None
    assert a.to_dict()['allocation_size'] is None


def test_excluded_assignment_record_has_unit_list():
    record = experiments(NAMESPACE).assign('checkout-color', {'organization_id': 16}).to_dict()
    assert record['arm'] is None
    assert record['excluded_by'] == 'checkout-copy'
    assert record['status'] == 'excluded'
    assert record['unit'] == ['organization_id']


def test_definition_revision_changes_with_arm_config():
    base = experiments(NAMESPACE).assign('checkout-color', {'organization_id': 1}).definition_revision
    override = {
        'experiment-layer.checkout': experiment_layer(
            unit=['organization_id'],
            experiments={'checkout-color': experiment(arms={'control': 50, 'treatment': 50}, start=0, size=40)},
        ),
    }
    with override_options(NAMESPACE, override):
        changed = experiments(NAMESPACE).assign('checkout-color', {'organization_id': 1}).definition_revision
    assert changed != base


def test_definition_revision_ignores_owner_and_description():
    base = experiment(arms={'control': 50, 'treatment': 50}, start=0, size=100)
    other = experiment(
        arms={'control': 50, 'treatment': 50}, start=0, size=100,
        team='growth', description='totally different',
    )
    with override_options(
        NAMESPACE, {
            'experiment-layer.checkout': experiment_layer(
                unit=['organization_id'], experiments={'checkout-color': base},
            ),
        },
    ):
        r1 = experiments(NAMESPACE).assign('checkout-color', {'organization_id': 1}).definition_revision
    with override_options(
        NAMESPACE, {
            'experiment-layer.checkout': experiment_layer(
                unit=['organization_id'], experiments={'checkout-color': other},
            ),
        },
    ):
        r2 = experiments(NAMESPACE).assign('checkout-color', {'organization_id': 1}).definition_revision
    assert r1 == r2


def test_restarting_with_old_settings_has_a_distinct_experiment_identity():
    records = []
    for experiment_name, model in [
        ('model-test-v1', 'model-a'),
        ('model-test-v2', 'model-b'),
        ('model-test-v3', 'model-a'),
    ]:
        definition = experiment(arms={'control': 1, 'treatment': 1}, size=100)
        definition['arms'][1]['config'] = {'model': model}
        with override_options(
            NAMESPACE, {
                'experiment-layer.checkout': experiment_layer(
                    unit=['organization_id'], experiments={experiment_name: definition},
                ),
            },
        ):
            assignment = experiments(NAMESPACE).assign(experiment_name, {'organization_id': 1})
            records.append(assignment.exposure('seer'))

    assert all(record['status'] == 'assigned' for record in records)
    assert len({record['subject'] for record in records}) == 1
    assert records[0]['definition_revision'] == records[2]['definition_revision']
    assert records[0]['definition_revision'] != records[1]['definition_revision']
    assert len({
        (record['namespace'], record['experiment'], record['subject'])
        for record in records
    }) == 3


def test_unassigned_has_no_definition_revision():
    a = experiments(NAMESPACE).assign('unconfigured-experiment', {'organization_id': 1})
    assert a.status == 'unassigned'
    assert a.definition_revision is None
    assert a.to_dict()['definition_revision'] is None


def test_layer_lists_members_in_slot_order():
    members = experiments(NAMESPACE).layer('checkout')
    assert members == [
        {'experiment': 'checkout-color', 'start': 0, 'size': 40, 'enabled': True},
        {'experiment': 'checkout-copy', 'start': 40, 'size': 30, 'enabled': True},
    ]
    assert experiments(NAMESPACE).layer('unknown') == []


def test_subject_encoding_does_not_collide_on_colon():
    override = {
        'experiment-layer.checkout': experiment_layer(
            unit=['u1', 'u2'],
            experiments={'checkout-color': experiment(arms={'a': 50, 'b': 50}, start=0, size=100)},
        ),
    }
    with override_options(NAMESPACE, override):
        checker = experiments(NAMESPACE)
        left = checker.assign('checkout-color', {'u1': 'a:b', 'u2': 'c'}).subject
        right = checker.assign('checkout-color', {'u1': 'a', 'u2': 'b:c'}).subject
    assert left != right
    assert left == '["a:b","c"]'
    assert right == '["a","b:c"]'


def test_string_unit_values_are_used_verbatim():
    a = experiments(NAMESPACE).assign('checkout-color', {'organization_id': '1'})
    assert a.slot == 6


def test_override_replaces_experiment_for_the_test():
    with override_options(
        NAMESPACE, {
            'experiment-layer.checkout': experiment_layer(
                unit=['organization_id'],
                experiments={'checkout-color': experiment(arms={'control': 0, 'treatment': 100}, start=70, size=30)},
            ),
        },
    ):
        a = experiments(NAMESPACE).assign('checkout-color', {'organization_id': 4})
        assert (a.status, a.arm) == ('assigned', 'treatment')
    assert experiments(NAMESPACE).assign('checkout-color', {'organization_id': 4}).status == 'holdout'


def test_override_validates_experiment_shape():
    with pytest.raises(Exception):
        with override_options(NAMESPACE, {'experiment-layer.checkout': {'unit': ['organization_id']}}):
            pass


def test_override_overlapping_layer_raises_and_restores():
    with pytest.raises(SchemaError, match='overlap'):
        with override_options(
            NAMESPACE, {
                'experiment-layer.checkout': experiment_layer(
                    unit=['organization_id'],
                    experiments={
                        'checkout-color': experiment(arms={'control': 50, 'treatment': 50}, start=0, size=50),
                        'checkout-copy': experiment(arms={'short': 50, 'long': 50}, start=40, size=30),
                    },
                ),
            },
        ):
            pass
    a = experiments(NAMESPACE).assign('checkout-color', {'organization_id': 1})
    assert (a.status, a.arm) == ('assigned', 'control')


def test_override_can_reallocate_siblings():
    with override_options(
        NAMESPACE, {
            'experiment-layer.checkout': experiment_layer(
                unit=['organization_id'],
                experiments={
                    'checkout-color': experiment(arms={'control': 50, 'treatment': 50}, start=0, size=50),
                    'checkout-copy': experiment(arms={'short': 50, 'long': 50}, start=50, size=30),
                },
            ),
        },
    ):
        assert experiments(NAMESPACE).layer('checkout') == [
            {'experiment': 'checkout-color', 'start': 0, 'size': 50, 'enabled': True},
            {'experiment': 'checkout-copy', 'start': 50, 'size': 30, 'enabled': True},
        ]
        a = experiments(NAMESPACE).assign('checkout-copy', {'organization_id': 16})
        assert (a.slot, a.status) == (58, 'assigned')
    assert experiments(NAMESPACE).layer('checkout') == [
        {'experiment': 'checkout-color', 'start': 0, 'size': 40, 'enabled': True},
        {'experiment': 'checkout-copy', 'start': 40, 'size': 30, 'enabled': True},
    ]


def test_assign_with_unconvertible_context_is_unassigned():
    a = experiments(NAMESPACE).assign('checkout-color', {'organization_id': float('nan')})
    assert a.status == 'unassigned'
    assert isinstance(a.reason, str) and a.reason
    with pytest.raises(ValueError):
        experiments(NAMESPACE).try_assign('checkout-color', {'organization_id': float('nan')})


def test_assign_rejects_oversized_int_context():
    huge = 2 ** 70
    assert experiments(NAMESPACE).assign('checkout-color', {'organization_id': huge}).status == 'unassigned'
    with pytest.raises(ValueError, match='organization_id'):
        experiments(NAMESPACE).try_assign('checkout-color', {'organization_id': huge})


def test_assign_accepts_max_i64_context_deterministically():
    ctx = {'organization_id': 2 ** 63 - 1}
    first = experiments(NAMESPACE).assign('checkout-color', ctx)
    second = experiments(NAMESPACE).assign('checkout-color', ctx)
    assert first.subject is not None
    assert (first.status, first.arm, first.slot, first.subject) == (
        second.status, second.arm, second.slot, second.subject,
    )


def test_assign_accepts_huge_int_passed_as_string():
    a = experiments(NAMESPACE).try_assign('checkout-color', {'organization_id': str(2 ** 70)})
    assert a.subject is not None


def test_documented_reference_hash_matches_extension():
    import hashlib

    def point(components: list[str]) -> int:
        h = hashlib.sha1()
        for c in components:
            b = c.encode()
            h.update(len(b).to_bytes(8, 'big'))
            h.update(b)
        return int.from_bytes(h.digest()[:8], 'big')

    def slot(components: list[str]) -> int:
        return point(components) % 100

    def arm(components: list[str], arms: list[tuple[str, int]]) -> str | None:
        total = sum(weight for _, weight in arms)
        p = point(components)
        cum = 0
        for name, weight in arms:
            cum += weight
            if p * total < (cum << 64):
                return name
        return None

    checkout = [('control', 50), ('treatment', 50)]

    org1_slot = slot(['layer', NAMESPACE, 'checkout', '1'])
    assert org1_slot == 6
    assert org1_slot == experiments(NAMESPACE).assign('checkout-color', {'organization_id': 1}).slot

    assert arm(['experiment', NAMESPACE, 'checkout-color', '5'], checkout) == 'treatment'
    assert experiments(NAMESPACE).assign('checkout-color', {'organization_id': 5}).arm == 'treatment'


def _assign_arms_for_weights(weights: dict[str, int], n: int = 2000) -> list[str | None]:
    override = {
        'experiment-layer.checkout': experiment_layer(
            unit=['organization_id'],
            experiments={'checkout-color': experiment(arms=weights, start=0, size=100)},
        ),
    }
    with override_options(NAMESPACE, override):
        checker = experiments(NAMESPACE)
        return [checker.assign('checkout-color', {'organization_id': org}).arm for org in range(n)]


def test_arm_selection_is_invariant_under_proportional_weight_scaling():
    even = _assign_arms_for_weights({'control': 50, 'treatment': 50})
    assert _assign_arms_for_weights({'control': 1, 'treatment': 1}) == even
    assert _assign_arms_for_weights({'control': 100, 'treatment': 100}) == even

    tilted = _assign_arms_for_weights({'a': 30, 'b': 70})
    assert _assign_arms_for_weights({'a': 3, 'b': 7}) == tilted
    assert _assign_arms_for_weights({'a': 300, 'b': 700}) == tilted


def test_testing_experiment_builder_defaults():
    value = experiment(arms={'control': 50, 'treatment': 50})
    assert value == {
        'owner': {'team': 'testing'},
        'allocation': {'start': 0, 'size': 20},
        'enabled': True,
        'arms': [
            {'name': 'control', 'weight': 50},
            {'name': 'treatment', 'weight': 50},
        ],
    }


def test_zero_size_is_rejected():
    with pytest.raises(SchemaError):
        with override_options(
            NAMESPACE, {
                'experiment-layer.checkout': experiment_layer(
                    unit=['organization_id'],
                    experiments={'checkout-color': experiment(arms={'control': 50, 'treatment': 50}, start=0, size=0)},
                ),
            },
        ):
            pass


def test_one_slot_size_is_accepted():
    with override_options(
        NAMESPACE, {
            'experiment-layer.checkout': experiment_layer(
                unit=['organization_id'],
                experiments={'checkout-color': experiment(arms={'control': 50, 'treatment': 50}, start=0, size=1)},
            ),
        },
    ):
        checker = experiments(NAMESPACE)
        assert checker.assign('checkout-color', {'organization_id': 1}) is not None
