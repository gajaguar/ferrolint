import abc
import dataclasses
import enum
import functools
from dataclasses import dataclass
from typing import ClassVar, Final
import typing
import os


class Base:
    baseAttr = 1
    def baseMethod(self): pass
    def base_method(self): pass
    @property
    def baseProp(self): return 1


class Child(Base):
    baseAttr = 2
    childAttr = 3
    CHILD_CONST = 4
    def baseMethod(self, camelArg): pass
    def newMethod(self, camelArg, _ok, a, B, *varArgs, kwOnly=1, **kwArgs): pass
    @property
    def propCamel(self): return 1
    @abc.abstractproperty
    def absProp(self): return 1
    @functools.cached_property
    def cachedProp(self): return 1
    @staticmethod
    def staticCamel(camelArg): pass
    @classmethod
    def classCamel(cls, camelArg): pass
    @baseProp.setter
    def baseProp(self, value): pass
    @propCamel.setter
    def propCamel(self, value): pass
    def posOnly(self, posArg, /, camelArg): pass
    def __DUNDER__(self): pass
    def __dunderOk__(self): pass
    def _privateCamel(self): pass
    def __mangledCamel(self): pass
    if True:
        def inIfCamel(self): pass
    lam = lambda lamArg: 1
    Final1: Final = 1
    final2: Final = 1
    final3: Final[int] = 1
    cv: ClassVar[int] = 1
    cv2: ClassVar[Final[int]] = 1
    cv3: typing.ClassVar[typing.Final] = 1
    selfAttr = None
    def __init__(self):
        self.instCamel = 1
        self.inst_ok = 2
        self.InstUp: Final = 3
        self.baseAttr = 1
        self.childAttr = 2
        self.other.camelX = 5
    def other_method(this):
        this.thisAttr = 1
    @classmethod
    def clsMeth(cls):
        cls.clsAttr = 1


@dataclass
class DC:
    camelField: int = 1
    CONST_FIELD: Final = 1
    camelFinal: Final = 1
    camelFinal2: Final[int] = 1


class Color(enum.Enum):
    RED = 1
    green = 2
    Blue = 3
    _hidden = 4
    __priv = 5
    camelMember: int = 6
    annOnly: int
    def meth(self): pass
    class Inner: pass
    callable_member = lambda self: 1
    tupleMember = (1, 2)
    aliasMember = RED
    _ignore_ = ["x"]


class Choice(enum.StrEnum):
    A_B = "a", "A"
    camelChoice = "c", "C"


class Flag2(enum.IntFlag):
    lowFlag = 1


class Meta:
    ordering = ["x"]
    verbose_name = "x"
    verboseNameCamel = "y"


def outer():
    counter = 0
    CamelOuter = 1
    def inner():
        nonlocal counter, CamelOuter
        counter = 1
        CamelOuter = 2
        newLocal = 3
    class Local:
        localAttr = 1
        def m(self):
            camelInM = 1
    return inner


import logging


class Handler(logging.Handler):
    def handleError(self, record): pass
    def ownMethod(self, Arg): pass
    def emit(self, Record): pass


class Plain(dict):
    def getItem(self, Key): pass
    def setdefault(self, Key, Default): pass


@dataclass
class Fields:
    camelField: int = 0

    def __init__(self):
        self.camelField = 1
        self.otherCamel = 2


import sys

if sys.version_info[0] == 3:
    Number = int
    from collections.abc import MutableSequence as Seq
else:
    Number = long
    from collections import MutableSequence as Seq


class Wide(Number):
    def __new__(Cls, Value): pass
    def ownWide(self, Arg): pass


class Many(Seq):
    def insert(self, Index, Value): pass
    def ownMany(self, Arg): pass


if sys.platform == "x":
    Stream = logging.StreamHandler[int]
else:
    Stream = logging.StreamHandler


class Sub(Stream):
    def handleError(self, Record): pass
    def ownSub(self): pass


class Wire:
    def __init__(self):
        self.size = 0


def build_wire():
    wire = Wire()
    wire.Size = 1
    wire.dwKind = 2
    wire.size = 3
    return wire


class Generic1(typing.Generic[typing.TypeVar("_Generic1")]):
    def feed(self, data, SEP=None):
        return data


class Narrow1(Generic1[int]):
    def feed(self, data, SEP=None):
        return data
