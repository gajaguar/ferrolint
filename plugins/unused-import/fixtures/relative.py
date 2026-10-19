"""Relative imports."""
from . import sibling
from .pkg import thing as other
from ..parent.module import helper
from . import used_sibling

print(used_sibling)


def inside_function():
    from . import local_other as renamed
    from .pkg import local_thing
