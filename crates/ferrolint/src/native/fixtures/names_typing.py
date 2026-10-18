import typing
from typing import Optional, List, Dict, Callable, Literal, Union, Tuple, Any, Type, TypeAlias, ClassVar, Final, Generic, TypeVar, NewType, ParamSpec, Protocol
import collections
import os

listT = List[int]
dictT = Dict[str, int]
callT = Callable[[int], int]
litT = Literal["a"]
tupT = Tuple[int, str]
anyT = Any
anyU = typing.Any
typeT = Type[int]
newT = NewType("newT", int)
NewT = NewType("NewT", int)
optT = typing.Optional[int]
unionT = typing.Union[int, str]
unionU = Union[int, str]
unionV = Union[int, str, None]
barT = int | None
barU = None | int
barV = list[int] | None
listL = list[int]
dictL = dict[str, int]
tupL = tuple[int, ...]
pspec = ParamSpec("pspec")
dd = collections.defaultdict(int)
nt = collections.namedtuple("nt", "a b")
Nt = collections.namedtuple("Nt", "a b")
od = collections.OrderedDict()
tupleCall = tuple([1])
class A: pass
class B(A): pass
sub = A | B
clsAlias = A
clsLower = int
x_y = A()
