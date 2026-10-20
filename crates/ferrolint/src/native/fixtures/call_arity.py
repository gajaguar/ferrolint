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
