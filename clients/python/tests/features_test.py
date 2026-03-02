from __future__ import annotations

import pytest
from sentry_options import FeatureContext
from sentry_options import features


NAMESPACE = 'sentry-options-testing'


def make_context(
    data: dict,
    identity_fields: list[str] | None = None,
) -> FeatureContext:
    return FeatureContext(data, identity_fields=identity_fields)


def test_features_returns_checker() -> None:
    checker = features(NAMESPACE)
    assert repr(checker) == 'FeatureChecker("sentry-options-testing")'


def test_feature_context_repr() -> None:
    ctx = make_context({'org_id': 1})
    assert repr(ctx) == 'FeatureContext(...)'


def test_feature_context_string_value() -> None:
    ctx = make_context({'slug': 'sentry'})
    checker = features(NAMESPACE)
    # No feature uses slug, just verify no error
    assert checker.has('organizations:enabled-feature', ctx) is False


def test_feature_context_accepts_all_scalar_types() -> None:
    ctx = make_context(
        {
            'str_field': 'hello',
            'int_field': 42,
            'float_field': 3.14,
            'bool_field': True,
        },
    )
    assert isinstance(ctx, FeatureContext)


def test_feature_context_accepts_list_types() -> None:
    ctx = make_context(
        {
            'str_list': ['a', 'b'],
            'int_list': [1, 2, 3],
            'float_list': [1.0, 2.5],
            'bool_list': [True, False],
        },
    )
    assert isinstance(ctx, FeatureContext)


def test_enabled_feature_matching_context_returns_true() -> None:
    ctx = make_context({'organization_id': 123})
    checker = features(NAMESPACE)
    assert checker.has('organizations:enabled-feature', ctx) is True


def test_enabled_feature_second_value_returns_true() -> None:
    ctx = make_context({'organization_id': 456})
    checker = features(NAMESPACE)
    assert checker.has('organizations:enabled-feature', ctx) is True


def test_enabled_feature_non_matching_context_returns_false() -> None:
    ctx = make_context({'organization_id': 999})
    checker = features(NAMESPACE)
    assert checker.has('organizations:enabled-feature', ctx) is False


def test_undefined_feature_returns_false() -> None:
    ctx = make_context({'organization_id': 123})
    checker = features(NAMESPACE)
    assert checker.has('organizations:nonexistent', ctx) is False


def test_missing_context_field_returns_false() -> None:
    # Feature condition checks organization_id, but context has no such field
    ctx = make_context({'user_id': 123})
    checker = features(NAMESPACE)
    assert checker.has('organizations:enabled-feature', ctx) is False


def test_disabled_feature_returns_false() -> None:
    # disabled-feature is enabled=False even though conditions would match
    ctx = make_context({'organization_id': 123})
    checker = features(NAMESPACE)
    assert checker.has('organizations:disabled-feature', ctx) is False


def test_rollout_zero_returns_false() -> None:
    # rollout-zero has rollout=0, so even matching conditions
    # always return false
    ctx = make_context({'organization_id': 123})
    checker = features(NAMESPACE)
    assert checker.has('organizations:rollout-zero', ctx) is False


def test_rollout_100_always_returns_true() -> None:
    # enabled-feature has rollout=100, so all matching contexts succeed
    for org_id in [123, 456]:
        ctx = make_context({'organization_id': org_id})
        checker = features(NAMESPACE)
        assert checker.has('organizations:enabled-feature', ctx) is True


def test_has_is_deterministic() -> None:
    # Same context produces same result on repeated calls
    ctx = make_context(
        {'organization_id': 123, 'user_id': 42},
        identity_fields=['organization_id', 'user_id'],
    )
    checker = features(NAMESPACE)
    result1 = checker.has('organizations:enabled-feature', ctx)
    result2 = checker.has('organizations:enabled-feature', ctx)
    assert result1 == result2


def test_identity_fields_affect_rollout_bucketing() -> None:
    # Two contexts with different identity values produce different ids,
    # which ensures rollout bucketing is identity-based, not random.
    ctx1 = make_context(
        {'organization_id': 123},
        identity_fields=['organization_id'],
    )
    ctx2 = make_context(
        {'organization_id': 999},
        identity_fields=['organization_id'],
    )
    checker = features(NAMESPACE)
    # enabled-feature only matches org 123/456 via conditions,
    # so ctx2 should be false
    assert checker.has('organizations:enabled-feature', ctx1) is True
    assert checker.has('organizations:enabled-feature', ctx2) is False


def test_feature_context_empty_dict() -> None:
    ctx = make_context({})
    checker = features(NAMESPACE)
    assert checker.has('organizations:enabled-feature', ctx) is False


def test_feature_context_bool_not_confused_with_int() -> None:
    # Python bool is a subclass of int; True should be stored as Bool, not Int
    ctx = make_context({'is_active': True})
    checker = features(NAMESPACE)
    # No feature checks is_active; just verify no type error occurs
    assert checker.has('organizations:enabled-feature', ctx) is False


def test_invalid_context_value_type_raises() -> None:
    with pytest.raises((TypeError, ValueError)):
        make_context({'key': object()})
