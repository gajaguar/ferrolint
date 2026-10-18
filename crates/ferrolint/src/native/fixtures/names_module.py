import os
import sys
from typing import TypeVar, Union, Final, TypeAlias, Optional, NamedTuple, TypedDict, Annotated
import typing
from enum import Enum

camelCase = 5
UPPER = 5
snake_case = 5
PascalCase = 5
c1 = [1]
c2 = {1: 2}
c3 = (1, 2)
c4 = f"x{os.sep}"
c5 = -1
c6 = 1 + 2
c7 = not True
c8 = [i for i in range(3)]
cL = lambda: 1
cLambda = lambda: 1
camelCall = os.getcwd()
camelUnknown = unknown_thing()
camelName = os
camelName2 = sys.argv
camelCls = Enum
camelSub = sys.argv[0]
camelIf = 1 if os else 2
camelComp = 1 < 2
camelBool = True and False
camelStr = "a" "b"
camelBytes = b"x"
camelNone = None
camelEllipsis = ...
camelObj = object()
camelDict = dict()
camelList = list()
camelSet = set()
camelInt = int("5")
camelStrCall = str(5)
camelLen = len("abc")
camelType = type("x", (), {})
camelType2 = type(5)
camelGet = os.environ.get("x")
camelJoin = os.path.join("a", "b")
camelOpen = open("x")
camelAwait = (yield_ := 5)
ann: int = 5
annC: int = 5
annNo: int
T = TypeVar("T")
Tx = TypeVar("Tx")
tv = TypeVar("tv")
TypeA = Union[int, str]
typeA = Union[int, str]
typeB = int | str
typeC = Optional[int]
TypeD: TypeAlias = int
typeE: TypeAlias = int
typeF: typing.TypeAlias = int
fin: Final = 3
Fin: Final = 3
a1, a2 = 1, 2
(bX, bY) = (1, 2)
[dX, dY] = [1, 2]
eX = eY = 4
class Foo: pass
fooAlias = Foo
FooAlias = Foo
fooInst = Foo()
FooInst = Foo()
def func(): pass
funcAlias = func
funcCall = func()
class Color(Enum):
    RED = 1
Colors = Color
colors = Color
NT = NamedTuple("NT", [("a", int)])
nt = NamedTuple("nt", [("a", int)])
TD = TypedDict("TD", {"a": int})
td = TypedDict("td", {"a": int})
aAnn = Annotated[int, 1]
AAnn = Annotated[int, 1]
STARRED: tuple = (1,)
STARRED += (*(k for k in STARRED),)
