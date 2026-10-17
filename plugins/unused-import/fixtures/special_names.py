"""__all__, del, string annotations and TYPE_CHECKING guards."""
from typing import TYPE_CHECKING, Optional
import typing
import os
import sys
import functools
import decimal
from math import pi as exported
from math import tau

if TYPE_CHECKING:
    from fractions import Fraction
    import numbers

if typing.TYPE_CHECKING:
    import statistics

__all__ = ["exported"]
__all__ += ["tau"]
label = "sys"
del os


@functools.cache
def cached(value: "Fraction") -> Optional["decimal.Decimal"]:
    return value


def annotated_local():
    import pathlib
    value: "pathlib.Path" = None
    return value
