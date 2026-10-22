"""Dummy names, type comments, one report per name, and class lookups."""
import a1
import a2
import a3
import a4
import typing  # noqa
from os import path as _path
from os import sep as unused_sep
from typing import Any, Optional
from functools import wraps
from json import loads as first
from pickle import loads as first_dummy_check
from json import loads as second
import sys

value = None  # type: typing.Optional[int]


def signature(a):
    # type: (Optional[int]) -> None
    pass


def dummy_local():
    from os import sep as _sep
    from os import curdir as keep
    import os as dummy_os


if sys:
    def wraps(f):
        return f
else:
    from functools import wraps


@a1.decorate
class Decorated:
    a1 = 1


class Reads:
    x = a2
    a2 = 3

    def method(self):
        return a3

    a3 = 1


class Defaults:
    def method(self, p=a4):
        pass

    a4 = 1


import a5
import a6
import typing.io


class SelfAssign:
    a5 = a5

    @a6.deco
    def a6(self):
        pass


def comment_only():
    # type: (typing.io.TextIO) -> None
    pass


def twice():
    from os import sep
    from os import sep as sep_again
    try:
        from json import dumps
    except ImportError:
        from simplejson import dumps


def annotated(a: "pathlib.Path | None", b: "decimal.Decimal"):
    import pathlib
    import decimal


if typing.TYPE_CHECKING:
    pass
else:
    from fractions import Fraction
