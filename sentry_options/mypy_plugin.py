from __future__ import annotations

from collections.abc import Callable
from typing import Any

from mypy.checker import TypeChecker
from mypy.errorcodes import ARG_TYPE
from mypy.nodes import StrExpr
from mypy.nodes import TypeInfo
from mypy.plugin import FunctionContext
from mypy.plugin import MethodContext
from mypy.plugin import Plugin
from mypy.types import Instance
from mypy.types import LiteralType
from mypy.types import Type

from sentry_options.api import _groups


def _1_string_arg(ctx: FunctionContext | MethodContext) -> str | None:
    (arg,), = ctx.args
    if isinstance(arg, StrExpr):
        return arg.value
    else:
        return None  # mypy has already `arg-type` errored for us!


def _adjust_option_group(ctx: FunctionContext) -> Type:
    group_name = _1_string_arg(ctx)
    if group_name is None:
        return ctx.default_return_type

    if group_name not in _groups:
        ctx.api.fail(
            f'unknown option group {group_name!r}',
            ctx.context,
            code=ARG_TYPE,
        )
        return ctx.default_return_type
    else:
        assert isinstance(ctx.api, TypeChecker)
        str_type = ctx.api.named_type('builtins.str')
        group_name_literal = LiteralType(group_name, fallback=str_type)
        assert isinstance(ctx.default_return_type, Instance)
        return ctx.default_return_type.copy_modified(args=[group_name_literal])


def _translate_type(ctx: MethodContext, tp: type[Any]) -> Type:
    # turn a runtime type into a typing type
    # fortunately we should only need to handle json compatible types!
    assert isinstance(ctx.api, TypeChecker)
    modpath = f'{tp.__module__}.{tp.__name__}'
    if tp.__module__ == 'builtins' and hasattr(tp, '__args__'):
        args = [_translate_type(ctx, tp) for tp in tp.__args__]
        return ctx.api.named_generic_type(modpath, args)
    elif tp.__module__ == 'builtins':
        return ctx.api.named_type(modpath)
    else:
        ti = ctx.api.modules[tp.__module__].names[tp.__name__].node
        assert isinstance(ti, TypeInfo)
        assert ti.typeddict_type is not None
        return ti.typeddict_type


def _adjust_option_value(ctx: MethodContext) -> Type:
    option_name = _1_string_arg(ctx)
    if option_name is None:
        return ctx.default_return_type

    assert isinstance(ctx.type, Instance)
    group_name_literal, = ctx.type.args
    if not isinstance(group_name_literal, LiteralType):
        return ctx.default_return_type  # an "error" OptionGroup[...]

    group_name = group_name_literal.value
    assert isinstance(group_name, str), group_name
    if option_name not in _groups[group_name]:
        ctx.api.fail(
            f'unknown option key {option_name!r} for group {group_name!r}',
            ctx.context,
            code=ARG_TYPE,
        )
        return ctx.default_return_type
    else:
        return _translate_type(ctx, _groups[group_name][option_name].tp)


class OptionsMypyPlugin(Plugin):
    def get_function_hook(
            self,
            fullname: str,
    ) -> Callable[[FunctionContext], Type] | None:
        if fullname == 'sentry_options.api.option_group':
            return _adjust_option_group
        else:
            return None

    def get_method_hook(
            self,
            fullname: str,
    ) -> Callable[[MethodContext], Type] | None:
        if fullname == 'sentry_options.api.OptionGroup.get':
            return _adjust_option_value
        else:
            return None


def plugin(version: str) -> type[OptionsMypyPlugin]:
    return OptionsMypyPlugin
