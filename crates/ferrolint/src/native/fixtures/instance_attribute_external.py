import sys


class IfSlots:
    errors = None
    if not sys.argv:
        __slots__ = ("a", "b")

    def __init__(self):
        self.a = 1
        self.b = 2
        self.errors = []
        self.zzz = 3


class IfElseSlots:
    if sys.argv:
        __slots__ = ("a", "b")
    else:
        __slots__ = ("c",)

    def __init__(self):
        self.a = 1
        self.b = 2
        self.c = 3
        self.d = 4


class TrySlots:
    try:
        __slots__ = ("a",)
    except ImportError:
        __slots__ = ("b",)

    def __init__(self):
        self.a = 1
        self.b = 1
        self.c = 1


class Alias:
    def __init__(self):
        self.a = 1

    def set(self):
        me = self
        me.b = 2
        me.c = 3

    def make(self):
        rv = object.__new__(self.__class__)
        rv.d = 1
        return rv

    @classmethod
    def build(cls):
        rv = cls.__new__(cls)
        rv.e = 1
        rv2 = Alias.__new__(Alias)
        rv2.f = 1
        rv3 = object.__new__(Alias)
        rv3.g = 1
        rv4: Alias = Alias.__new__(Alias)
        rv4.h = 1
        return rv


def func():
    x = Alias()
    x.i = 2
    y = Alias.__new__(Alias)
    y.j = 3


class Other:
    def __init__(self):
        self.o = 1


class Mismatch:
    def __init__(self):
        self.a = 1

    def make(self):
        rv = Other.__new__(Mismatch)
        rv.b = 2
        rv2 = Mismatch.__new__(Other)
        rv2.c = 3
        rv3 = object.__new__(Other)
        rv3.d = 4


class TypeSelf:
    def __init__(self):
        self.a = 1

    def make(self):
        rv = object.__new__(type(self))
        rv.b = 1
        rv2 = type(self)()
        rv2.c = 1
        rv3 = self.__class__()
        rv3.d = 1


class Nested:
    def __init__(self):
        self.a = 1

    def run(self):
        me = self

        def inner():
            me.b = 1

        return inner

    def later(self):
        other = self
        other.c = 1
        yield 1
        other.d = 2
