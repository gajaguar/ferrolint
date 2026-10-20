"""One edge imported twice, once under TYPE_CHECKING."""
from typing import TYPE_CHECKING

from top import m2

if TYPE_CHECKING:
    from top import m2 as again
