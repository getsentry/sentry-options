from __future__ import annotations

from typing import Union

_Primitive = Union[str, int, float, bool]
_Object = dict[str, _Primitive]
OptionValue = Union[_Primitive, _Object, list[Union[_Primitive, _Object]]]

def init() -> None: ...
"""
Initialize the options extension with schema and
values defined in environment variables or production paths.
"""

def options(namespace: str) -> NamespaceOptions: ...
"""Create NamespaceOptions for a given options namespace"""

def features(namespace: str) -> FeatureChecker: ...
"""Create a FeatureChecker for a given options namespace"""

class NamespaceOptions:
    def get(self, key: str) -> OptionValue: ...
    """Get the value for a named option. If no value is defined the default will be returned"""

    def isset(self, key: str) -> bool: ...
    """See if an option is defined and has a value set."""

    def __repr__(self) -> str: ...


class FeatureChecker:
    """
    Interface for checking features flags against a context object.
    """
    def has(self, feature_name: str, context: FeatureContext) -> bool:
        """Check if a feature flag with `feature_name` is available to `context`"""
    def __repr__(self) -> str: ...


class FeatureContext:
    """
    A container of context data used to check feature flags.
    """
    def __init__(self, data: dict[str, OptionValue], *, identity_fields: list[str] | None = None) -> None: ...
    """
    Constructor

    Parameters
    ----------

    data: dict[str, OptionValue]
        The context data dictionary
    identity_fields: list[str] | None
        The fields that should be used to compute the 'identity' of a context object.
        The calculated identity will be used to determine rollout groups and typically
        contains the identifiers for a specific user/organization
    """

    def __repr__(self) -> str: ...

class OptionsError(Exception): ...
class SchemaError(OptionsError): ...
class UnknownNamespaceError(OptionsError): ...
class UnknownOptionError(OptionsError): ...
class NotInitializedError(OptionsError): ...

def _set_override(namespace: str, key: str, value: OptionValue) -> OptionValue | None: ...
def _clear_override(namespace: str, key: str) -> None: ...
def _validate_option(namespace: str, key: str, value: OptionValue) -> None: ...
