"""Quoted annotations, elif TYPE_CHECKING, nested __all__ and semicolons."""
import sys
from typing import TYPE_CHECKING, Optional
from math import pi, tau, e
from decimal import Decimal
import os; import string

if sys.version_info >= (3, 11):
    pass
elif TYPE_CHECKING:
    import numbers

try:
    __all__ = ["pi"]
except ImportError:
    pass

__all__: list[str] = ["tau"]


def annotated(value: "e") -> Optional["Decimal"]:
    return value


for item in range(2):
    import gzip
    print(item)
else:
    import zlib
