import sys
import os
import unittest
from typing import NoReturn


def a():
    return 1
    x = 2


def b():
    raise ValueError
    yield
    y = 1


def c():
    for i in range(3):
        continue
        print(i)
    while True:
        break
        print(1)


def d():
    sys.exit(1)
    print("x")


def e():
    exit()
    print("x")


def f():
    os._exit(1)
    print("x")


def die() -> NoReturn:
    raise SystemExit


def g():
    die()
    print("x")


class T(unittest.TestCase):
    def test(self):
        self.fail("x")
        print(1)


def h():
    if True:
        return 1
        print(2)
    else:
        raise ValueError
        print(3)


@property
def i():
    return 1

    @decorator
    def inner():
        pass


def j(x):
    if x:
        return 1


def k(x):
    if x:
        return 1
    else:
        return


def l(x):
    if x:
        return 1
    return


def m(x):
    if x:
        return 1
    elif x == 2:
        return 2


def n(x):
    if x:
        return 1
    elif x == 2:
        return 2
    return 3


def o(x):
    while True:
        if x:
            return 1


def p(x):
    while True:
        if x:
            return 1
        break


def q(x):
    try:
        return 1
    except ValueError:
        pass


def r(x):
    try:
        return 1
    except ValueError:
        return 2


def s(x):
    try:
        raise ValueError
    except ValueError:
        pass
    return 1


def t(x):
    if x:
        return 1
    raise ValueError


def u(x):
    if x:
        return 1
    sys.exit(2)


def v(x):
    if x:
        return 1
    assert False


async def w(x):
    if x:
        return 1


def useless1():
    print()
    return


def useless2():
    print()
    return None


def useless3(x):
    if x:
        print()
    else:
        return None


def useless4(x):
    try:
        print()
    finally:
        return


def useless5():
    return


def useless6(x):
    for i in x:
        return None


def returns(x):
    if x == 1:
        return 1
    if x == 2:
        return 2
    if x == 3:
        return 3
    if x == 4:
        return 4
    if x == 5:
        return 5
    if x == 6:
        return 6
    return 7


def args(a, b, c, d, e, f):
    pass


def args2(a, b, c, d, e, _f, unused_g, ignored_h, i=1):
    pass


class K:
    def meth(self, a, b, c, d, e):
        pass

    def meth2(self, a, b, c, d, e, f):
        pass

    @staticmethod
    def meth3(a, b, c, d, e, f):
        pass

    @classmethod
    def meth4(cls, a, b, c, d, e):
        pass

    def meth5(self, a, b, c, *, d, e, f):
        pass


def vararg(a=1, *args):
    pass


def vararg2(a, /, *args):
    pass


def vararg3(a=1, /, b=2, *args):
    pass


def vararg4(a=1, /, *args):
    pass


def locs():
    a1 = b1 = c1 = d1 = e1 = f1 = g1 = h1 = 1
    i1, j1 = 1, 2
    for k1 in []:
        pass
    with open("x") as l1:
        pass
    try:
        pass
    except Exception as m1:
        pass
    import os as n1
    from sys import path as o1
    p1: int
    q1: int = 1
    [r1 for r1 in []]
    (s1 := 1)
    def t1(): pass
    class u1: pass
    del a1


def docstring_a():
    """doc"""
    return


def docstring_b():
    """doc"""
    print()
    return


def docstring_c():
    """doc"""
    return None


def docstring_d():
    "doc"
