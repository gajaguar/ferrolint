import collections
import collections as coll
from collections import namedtuple
from collections import namedtuple as nt
from collections import OrderedDict
import typing
from typing import NamedTuple

Point = namedtuple("Point", "x y")
Pair = nt("Pair", ["a", "b"])
Triple = collections.namedtuple("Triple", ["a", "b", "c"])
Quad = coll.namedtuple(
    "Quad",
    ["a", "b", "c", "d"],
)
Typed = NamedTuple("Typed", [("a", int)])
Typed2 = typing.NamedTuple("Typed2", [("a", int)])
Ordered = OrderedDict()
Other = collections.defaultdict(list)


class Derived(namedtuple("Derived", "a b")):
    pass


class Declared(NamedTuple):
    a: int


def function():
    from collections import namedtuple as local

    return local("Local", "a")


def shadowed(namedtuple):
    return namedtuple("Shadowed", "a")


values = [namedtuple("Item", "a") for _ in range(2)]
