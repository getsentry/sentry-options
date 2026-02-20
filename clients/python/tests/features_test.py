from __future__ import annotations

from sentry_options import FeatureContext
from sentry_options import features


def test_feature_context_empty() -> None:
    ctx = FeatureContext({})
    assert ctx is not None


def test_feature_context_with_string_value() -> None:
    ctx = FeatureContext({'organization_slug': 'sentry'})
    assert ctx is not None


def test_feature_context_with_int_value() -> None:
    ctx = FeatureContext({'user_id': 456})
    assert ctx is not None


def test_feature_context_with_float_value() -> None:
    ctx = FeatureContext({'score': 0.75})
    assert ctx is not None


def test_feature_context_with_bool_value() -> None:
    ctx = FeatureContext({'is_free': True})
    assert ctx is not None


def test_feature_context_with_list_value() -> None:
    ctx = FeatureContext({'org_ids': [1, 2, 3]})
    assert ctx is not None


def test_feature_context_with_identity_fields() -> None:
    ctx = FeatureContext(
        {'user_id': 456, 'organization_id': 123},
        identity_fields=['user_id', 'organization_id'],
    )
    assert ctx is not None


def test_feature_context_bool_not_treated_as_int() -> None:
    # True is a bool in Python (subclass of int); must be stored as Bool
    # not Int. If incorrectly stored as Int, an equals condition against
    # json(true) would fail
    ctx = FeatureContext({'is_free': True})
    assert ctx is not None


def test_has_unknown_feature_returns_false() -> None:
    fc = features('sentry-options-testing')
    ctx = FeatureContext({'organization_slug': 'sentry'})
    assert fc.has('organizations:nonexistent', ctx) is False


def test_has_unknown_namespace_returns_false() -> None:
    fc = features('nonexistent-namespace')
    ctx = FeatureContext({'organization_slug': 'sentry'})
    assert fc.has('organizations:purple-site', ctx) is False


def test_has_matching_context_returns_true() -> None:
    fc = features('sentry-options-testing')
    ctx = FeatureContext({'organization_slug': 'sentry'})
    assert fc.has('organizations:purple-site', ctx) is True


def test_has_missing_context_field_returns_false() -> None:
    fc = features('sentry-options-testing')
    ctx = FeatureContext({})  # no organization_slug
    assert fc.has('organizations:purple-site', ctx) is False


def test_has_context_field_present_but_no_match_returns_false() -> None:
    fc = features('sentry-options-testing')
    ctx = FeatureContext({'organization_slug': 'other-org'})
    assert fc.has('organizations:purple-site', ctx) is False


def test_has_disabled_feature_returns_false() -> None:
    fc = features('sentry-options-testing')
    ctx = FeatureContext({'organization_slug': 'sentry'})
    assert fc.has('organizations:disabled-flag', ctx) is False


def test_has_no_segments_returns_false() -> None:
    fc = features('sentry-options-testing')
    ctx = FeatureContext({'organization_slug': 'sentry'})
    assert fc.has('organizations:no-segments', ctx) is False


def test_has_rollout_is_deterministic() -> None:
    fc = features('sentry-options-testing')
    ctx = FeatureContext(
        {'organization_slug': 'sentry'},
        identity_fields=['organization_slug'],
    )
    result = fc.has('organizations:purple-site', ctx)
    for _ in range(10):
        assert fc.has('organizations:purple-site', ctx) == result
