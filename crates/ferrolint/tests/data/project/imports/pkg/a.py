"""Imports that resolve, and imports that do not."""
import pkg.b
from pkg import b, c, nope
from . import b as other
from . import nope as missing
from .b import VALUE, nope2
from .sub import deep, nope3
from .sub.deep import VALUE as DEEP, absent
from .missing import thing
from .sub.nothing import thing2
from ... import far
from .. import up
from pkg.sub.deep import VALUE as AGAIN
