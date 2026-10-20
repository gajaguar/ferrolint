"""A missing relative module goes unreported where it cannot fail."""
import sys
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .gone import types_only

if sys.version_info > (3,):
    from .gone import by_version

try:
    from .gone import optional
except ImportError:
    optional = None

try:
    from .gone import other
except ValueError:
    other = None
