import os
import sys
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    TcName = 1
    tc_name = 2

try:
    import ujson as json
    jsonLib = 1
except ImportError:
    import json
    jsonLib = 2
else:
    elseVar = 3
finally:
    finVar = 4

try:
    optionalFlag = True
except ImportError:
    optionalFlag = False

x_a = x_b = 5
xA = xB = 6
first, *rest = 1, 2, 3
firstA, *restB = [1, 2, 3]
(tupA, tupB) = 1, 2
[listA, listB] = 1, 2
counter = 0
counter += 1
Counter2 = 0
Counter2 += 1
del counter
lst = []
lst.append(1)
dct = {}
dct["k"] = 1
SOME_LIST = []
SOME_LIST += [1]
walrus_mod = (wal := 5)
if (condVar := 5):
    pass
for ii in range(3):
    pass
for jj, kk in [(1, 2)]:
    pass
with open("f") as fileObj:
    pass
if os.name == "nt":
    winVar = 1
elif os.name == "x":
    xVar = 2
else:
    otherVar = 3
if os.name == "nt":
    sameVar = 1
elif os.name == "x":
    sameVar = 2
else:
    sameVar = 3
if sys.argv:
    brokenVar = 1
    brokenVar = 2
match sys.argv:
    case [capA, *capB]:
        pass
    case _:
        pass


def func(arg_a, argB=5, *argsC, kwD=4, **kwE):
    global gVar
    gVar = 5
    for loopV in range(3):
        pass
    with open("f") as withV, open("g") as withW:
        pass
    try:
        pass
    except (ValueError, TypeError) as excE:
        pass
    a, (b, c) = 1, (2, 3)
    d = e = 5
    [f, g] = 1, 2
    argB = 6
    arg_a += 1
    lam = lambda lamArg, lamB=(lamWal := 5): lamArg
    del d
    if (wal2 := 5):
        pass
    x = [y for y in range(3) if (zz := y)]
    return a, b, c, e, f, g, lam, x


async def afunc(selfArg):
    async with open("f") as asyncWith:
        pass
    async for asyncFor in selfArg:
        pass
    asyncRes = [i async for i in selfArg]
    return asyncRes


class K:
    clsA = 1
    for clsLoop in range(3):
        pass
    with open("f") as clsWith:
        pass
    try:
        pass
    except Exception as clsExc:
        pass
    clsLam = lambda clsLamArg: 1
    (clsT1, clsT2) = (1, 2)
    clsComp = [clsI for clsI in range(3)]
    def method(self, *, kwOnly=1):
        self.attr_a: int = 1
        self.attrB, self.attrC = 1, 2
        self.attrD = self.attrE = 3
        for self.attrF in range(3):
            pass
        with open("f") as self.attrG:
            pass
        self.attr_a += 1
        self.attrH += 1
        (self.attrI, self.attrJ) = 1, 2

    def other(self):
        def nested(selfLike):
            selfLike.nestedAttr = 1
        self.attr_a = 2
        self.attrK = 5


@staticmethod
def deco(): pass


def _private(): pass
def __very_private(): pass
def CamelFunc(): pass
def camelFunc(): pass
def fUNC(): pass
def func_1(): pass
def func1(): pass
def Func_Name(): pass
def _(): pass
def __(): pass
def ___(): pass
def _a_B(): pass
def __dunder__(): pass
def __Dunder__(): pass
def __dunder_x__(): pass
class lower_class: pass
class _PrivateClass: pass
class __PrivateClass: pass
class Class_Name: pass
class CLASS: pass
class C: pass
class Class1: pass
class A1b2: pass


from typing import TypeVar


def generic():
    FooT = TypeVar("FooT")
    BadT = 1
    return FooT, BadT
