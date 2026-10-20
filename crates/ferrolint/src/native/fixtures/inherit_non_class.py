from typing import Protocol
from somewhere import Unknown


def func():
    pass


async def afunc():
    pass


lam = lambda: 1
number = 1
text = "x"
items = [1]
pair = (1, 2)
mapping = {}
nothing = None
inst = Unknown()
twice = 1
twice = 2
flag = True


class Real:
    pass


made = Real()


@property
def decorated():
    pass


class A1(func):
    pass


class A2(afunc):
    pass


class A3(lam):
    pass


class A4(number):
    pass


class A5(text):
    pass


class A6(items):
    pass


class A7(pair):
    pass


class A8(mapping):
    pass


class A9(nothing):
    pass


class B3(bool):
    pass


class B4(range):
    pass


class B5(slice):
    pass


class B6(memoryview):
    pass


class B7(zip):
    pass


class B8(map):
    pass


class B9(inst):
    pass


class C2(made):
    pass


class C3(decorated):
    pass


class C4(1):
    pass


class C5("x"):
    pass


class C6(None):
    pass


class C7(Real, func):
    pass


class C9(Unknown, Real):
    pass


class D1(flag):
    pass


class D2(Real, metaclass=lam):
    pass


class D3(print):
    pass


class D4(len):
    pass


class D5(Protocol):
    pass


class D6(Real()):
    pass


class D7(func()):
    pass


class D8([1]):
    pass


class E1(Unknown.attr):
    pass


class E3(lambda: 1):
    pass


class E4(type(Real)):
    pass


class E5(NotImplemented):
    pass


class E6(Ellipsis):
    pass


class E7(__builtins__):
    pass


class E8(__name__):
    pass


class E9(int, func):
    pass
