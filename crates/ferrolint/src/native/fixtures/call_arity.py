import functools


class Base:
    def m(self, a):
        pass


class Sub(Base):
    def m(self, a, b):
        pass

    @staticmethod
    def s(a, b):
        pass

    @staticmethod
    def s2(a):
        pass

    def inst(self):
        self.later = None

    def later(self, a):
        pass

    def __init__(self, a):
        pass

    def __len__(self):
        return 0

    def __priv(self, a):
        pass

    @property
    def p(self):
        return self.m(1)

    def run(self):
        self.m(1)
        self.s(1)
        self.s2(1, 2)
        self.later()
        self.m(1, 2, 3)
        self.m(1, a=2)
        self.m(1, b=2, c=3)
        self.s(a=1)
        self.__init__()
        self.__init__(1, 2)
        self.__len__(1)
        self.__priv()
        self.m(1, 2)
        self.undefined()
        print(self.m(1), [self.m(i) for i in range(3)])

    def shadow(self):
        def m(a):
            pass

        m()
        m(1)
        self.m(2)


class Plain:
    def __init__(self, a, *, k):
        pass


class Derived(Plain):
    def __init__(self, a, b):
        pass


class Raised(Exception):
    def __init__(self, a):
        pass


class WithNew:
    def __new__(cls, a):
        return super().__new__(cls)

    def __init__(self, a, b):
        pass


class NoInit:
    pass


class Aliased:
    def f(self, a):
        pass

    g = f

    def run(self):
        self.f()


class Outer:
    def f(self, a):
        pass

    class Inner:
        def run(self):
            self.f()

    def run(self):
        self.f(a=1, a2=2)
        self.f(1, a=1)
        self.f(self=1)


def f(a, b=1, *, c):
    pass


def g(a, /, b):
    pass


def only_kw(*, a, b=1):
    pass


def varied(a, b=1, *args, c, **kw):
    pass


def free(*args, **kwargs):
    pass


def with_self(self, a):
    pass


def two(a, b):
    pass


early(1)


def early(a, b):
    pass


def late(a):
    return early(a)


if functools:
    def cond(a):
        pass
else:
    def cond(a, b):
        pass


def wrapper(*args, **kwargs):
    early(args)
    early(*args)
    early(**kwargs)
    early(1, **kwargs)
    early(1, *args)
    return lambda: early(1)


Plain()
Plain(1)
Plain(1, 2, k=1)
Derived(1)
Derived(1, 2)
Raised()
WithNew(1)
NoInit(1)
f()
f(1, 2, 3)
f(1, a=2, c=1)
f(1, d=2)
g(a=1, b=2)
g(1)
only_kw(1)
only_kw(b=2)
only_kw(a=1, c=2)
only_kw(a=1, b=2)
varied(1, 2, 3, 4, c=1, z=2)
varied()
varied(b=1)
varied(1, c=1, a=2)
free()
free(1, 2, a=3)
with_self()
with_self(1, self=2)
two(1)
two(1, 2)
two(1, 2, 3)
two(b=1)
two(1, b=2)
two(1, 2, b=3)
cond(1, 2, 3)
x = lambda a: two(a)
import ctypes
from ctypes import c_ulong

ctypes.c_int()
c_ulong()
c_ulong(3)
c_ulong(value=3)
ctypes.c_char_p()
import ctypes.wintypes
from ctypes.wintypes import DWORD

ctypes.wintypes.WORD()
DWORD()
DWORD(1)


from helpers import wrapper


@wrapper
def decorated(a, b):
    pass


class Decorated:
    @wrapper
    def __init__(self, a):
        pass

    @wrapper
    def m(self, a):
        pass

    @property
    def p(self):
        pass

    def call(self):
        self.m()
        self.m(b=1)
        self.p()


decorated()
decorated(1, 2, 3)
decorated(1, 2, c=3)
Decorated()
Decorated(b=1)


class NewOfObject:
    def copy(self):
        return self.__new__(NewOfObject)

    def two(self):
        return self.__new__(NewOfObject, 1)

    def own(self):
        return object.__new__(NewOfObject)


class Factory:
    def __init__(self, a):
        pass

    @classmethod
    def make(cls):
        return cls(), cls(1), cls(b=2)

    @classmethod
    def nested(cls):
        def inner(cls):
            return cls()

        return inner

    def instance(cls):
        return cls()


class WithBase(Factory):
    def copy(self):
        return self.__new__(WithBase)

    def own(self):
        return self.__new__(WithBase, 1)


def descriptors():
    import functools

    def plain(a, b=1, *args, c, **kw):
        return a, b, args, c, kw

    def posonly(a, b, /):
        return a, b

    @staticmethod
    def wrapped(a):
        return a

    @functools.wraps(plain)
    def wraps(a):
        return a

    @classmethod
    def method(cls):
        return cls

    wrapped.__get__(None)
    wraps.__get__(None)
    method.__get__(None)
    plain.__get__(None, int)
    plain.__get__(None, int, 1)
    plain.__get__(None, int, 1, 2)
    posonly.__get__(None, int)


def unpacked(a, b):
    return a, b


def star():
    empty = []
    empty.append(1)
    unpacked(*empty)
    one = [1]
    unpacked(*one)
    unpacked(*one, 2)
    unpacked(*one, b=2)
    triple = (1, 2, 3)
    unpacked(*triple)
    many = []
    if empty:
        many = [1, 2]
    unpacked(*many)
    unknown = [*one]
    unpacked(*unknown)


class Meta(type):
    def init(cls, name):
        cls.setup(name)
        cls.setup(cls, name)
        cls.fixed(name)
        cls.fixed()

    def other(self):
        self.setup()

    @classmethod
    def build(mcs):
        mcs.setup(1)

    def setup(cls, name):
        return name

    @staticmethod
    def fixed(name):
        return name


class Ordinary:
    def init(cls, name):
        cls.setup(name)

    def setup(cls, name):
        return name


def register(cls):
    return cls


@register
class Registered:
    def __init__(self, a):
        pass


Registered()
