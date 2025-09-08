from __future__ import annotations

import pathlib

import pytest

from sentry_options.write import _check_overlap
from sentry_options.write import _FileData
from sentry_options.write import _load
from sentry_options.write import _validate
from sentry_options.write import main


def test_validate_not_a_dict(tmp_path: pathlib.Path) -> None:
    fname = tmp_path.joinpath('t.yaml')
    fname.write_text('1')

    with pytest.raises(SystemExit) as excinfo:
        _validate('testing', str(fname))
    msg, = excinfo.value.args
    assert msg == f'expected top-level mapping: {fname}'


@pytest.mark.parametrize('s', ('{a: 1}', '{options: {}, a: 2}'))
def test_validate_not_options_dict(tmp_path: pathlib.Path, s: str) -> None:
    fname = tmp_path.joinpath('t.yaml')
    fname.write_text(s)

    with pytest.raises(SystemExit) as excinfo:
        _validate('testing', str(fname))
    msg, = excinfo.value.args
    assert msg == f'expected top-level `options:`: {fname}'


def test_validate_options_not_a_dict(tmp_path: pathlib.Path) -> None:
    fname = tmp_path.joinpath('t.yaml')
    fname.write_text('options: 1')

    with pytest.raises(SystemExit) as excinfo:
        _validate('testing', str(fname))
    msg, = excinfo.value.args
    assert msg == f'expected top-level `options:` dict: {fname}'


def test_validate_not_registered_option(tmp_path: pathlib.Path) -> None:
    fname = tmp_path.joinpath('t.yaml')
    fname.write_text('options: {unknown: 1}')

    with pytest.raises(SystemExit) as excinfo:
        _validate('testing', str(fname))
    msg, = excinfo.value.args
    assert msg == f'unknown option `unknown` (in group `testing`): {fname}'


def test_validate_incorrect_option_type(tmp_path: pathlib.Path) -> None:
    fname = tmp_path.joinpath('t.yaml')
    fname.write_text('options: {example-option: 1}')

    with pytest.raises(SystemExit) as excinfo:
        _validate('testing', str(fname))
    msg, = excinfo.value.args
    assert msg == (
        f'incorrect option type (`[testing]example-option`): '
        f"expected `<class 'str'>` got `1`: {fname}"
    )


def test_check_overlap_trivial() -> None:
    _check_overlap({})


def test_check_overlap_no_overlap() -> None:
    _check_overlap({
        'testing': {
            'us': [
                _FileData('1.yaml', {'example-option': '1'}),
                _FileData('2.yaml', {'float-option': 1.0}),
            ],
        },
    })


def test_check_overlap_with_overlap() -> None:
    with pytest.raises(SystemExit) as excinfo:
        _check_overlap({
            'testing': {
                'us': [
                    _FileData('1.yaml', {'example-option': '1'}),
                    _FileData('2.yaml', {'example-option': '2'}),
                ],
            },
        })
    msg, = excinfo.value.args
    assert msg == (
        "key(s) ['example-option'] in testing/us are defined by both "
        '1.yaml and 2.yaml'
    )


def test_load_rejects_yml_files(tmp_path: pathlib.Path) -> None:
    yml = tmp_path.joinpath('testing/us/1.yml')
    yml.parent.mkdir(parents=True)
    yml.write_text('options: {}')

    with pytest.raises(SystemExit) as excinfo:
        _load(str(tmp_path))
    msg, = excinfo.value.args
    assert msg == f'expected .yaml file: {yml}'


def test_load_rejects_incorrect_structure(tmp_path: pathlib.Path) -> None:
    wrong = tmp_path.joinpath('wrong.yaml')
    wrong.write_text('options: {}')

    with pytest.raises(SystemExit) as excinfo:
        _load(str(tmp_path))
    msg, = excinfo.value.args
    assert msg == f'expected {{group}}/{{target}}/{{file}}.yaml: {wrong}'


def test_load_rejects_unknown_group(tmp_path: pathlib.Path) -> None:
    yml = tmp_path.joinpath('unknown-group/us/1.yaml')
    yml.parent.mkdir(parents=True)
    yml.write_text('options: {}')

    with pytest.raises(SystemExit) as excinfo:
        _load(str(tmp_path))
    msg, = excinfo.value.args
    assert msg == f'unexpected group: unknown-group: {yml}'


def test_load_ignores_non_yaml_files(tmp_path: pathlib.Path) -> None:
    us_d = tmp_path.joinpath('testing/us')
    us_d.mkdir(parents=True)
    us_d.joinpath('README.md').write_text('us!\n')
    opts = us_d.joinpath('1.yaml')
    opts.write_text('options: {example-option: "1"}')

    assert _load(str(tmp_path)) == {
        'testing': {'us': [_FileData(str(opts), {'example-option': '1'})]},
    }


def test_error_if_output_exists(tmp_path: pathlib.Path) -> None:
    in_d = tmp_path.joinpath('in')
    out_d = tmp_path.joinpath('out')
    out_d.mkdir()

    with pytest.raises(SystemExit) as excinfo:
        main(('--root', str(in_d), '--out', str(out_d)))
    msg, = excinfo.value.args
    assert msg == f'output already exists: {out_d}'


def test_merges_options(tmp_path: pathlib.Path) -> None:
    in_d = tmp_path.joinpath('in')

    default_d = in_d.joinpath('testing/default')
    default_d.mkdir(parents=True)
    default_d.joinpath('1.yaml').write_text('options: {example-option: "1"}')

    us_d = in_d.joinpath('testing/us')
    us_d.mkdir(parents=True)
    us_d.joinpath('1.yaml').write_text('options: {example-option: "2"}')
    us_d.joinpath('2.yaml').write_text('options: {float-option: 0.5}')

    out_d = tmp_path.joinpath('out')

    assert not main(('--root', str(in_d), '--out', str(out_d)))

    data = out_d.joinpath('sentry-options-testing-us.json').read_text()
    assert data == '{"options":{"example-option":"2","float-option":0.5}}'
