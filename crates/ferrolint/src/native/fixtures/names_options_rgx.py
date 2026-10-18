import functools
import abc
import enum
from typing import TypeVar, ParamSpec, TypeVarTuple, TypeAlias, Final, Union

ab_cd = 1
abcd = 2
tv = TypeVar("tv")
TV = TypeVar("TV")
ps = ParamSpec("ps")
tvt = TypeVarTuple("tvt")
ta: TypeAlias = int
TA: TypeAlias = int
tu = Union[int, str]


def fn_ok(a_x, bad):
    v_ok = 1
    badvar = 2
    return [it for it in range(3)] + [iv for iv in range(3)]


class ab1:
    ca_x = 1
    bad_ca = 2
    cc_x: Final = 1
    cc_bad: Final = 2

    def m_ok(self):
        self.at_x = 1
        self.bad_at = 2

    @functools.cached_property
    def prop_cached(self):
        return 1

    @abc.abstractproperty
    def prop_abs(self):
        return 1

    @property
    def Prop(self):
        return 1


class Bad:
    pass


class Col(enum.Enum):
    cc_ok = 1
    other = 2


type PEP = int
def gen[X, *Y, **Z](x): pass
class G[Q]: pass
