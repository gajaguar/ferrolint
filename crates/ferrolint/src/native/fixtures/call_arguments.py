def f(a, b):
    return a + b


def g(a, b, /, **kw):
    return a, b, kw


def h(a, /, b, *, c, **kw):
    return a, b, c, kw


def k(a, b=1):
    return a, b


def vararg(a, b, *args):
    return a, b, args


def kwonly(a, *, b):
    return a, b


def main(a, b, c):
    f(a, b)
    f(b, a)
    f(b=b, a=a)
    f(b, a=a)
    f(a, b=b)
    f(a, c)
    f(b, 1)
    f(b, a, c)
    f(a, a)
    f(b, a, a)
    f(*[a, b])
    f(**{"a": a})
    g(a, b, a=1)
    g(a, b, b=2, c=3)
    g(a, b, a=1, a=2)
    h(a, b, c=c, a=1)
    h(a, b, c=c, d=2)
    k(b, a)
    k(b, a=a)
    vararg(b, a)
    kwonly(b, a)
    kwonly(b, b=a)
    kwonly(a, b=b)
    kwonly(b, a=a)


class C:
    def __init__(self, a, b):
        self.a = a

    def m(self, a, b):
        pass

    def n(self, a, /, **kw):
        pass

    @classmethod
    def c(cls, a, b):
        pass

    @staticmethod
    def s(a, b):
        pass

    def run(self, a, b, cls):
        self.m(b, a)
        self.m(a, b)
        self.n(a, a=2)
        self.n(1, a=2)
        self.n(1, self=2)
        self.c(b, a)
        self.s(b, a)
        C(b, a)
        C(a, b)
        C(b=b, a=a)
        C.m(self, b, a)
        self.m(b, a)

        def inner(a, b):
            pass

        inner(b, a)


def shadow(f, a, b):
    f(b, a)


def late(a, b):
    f(b, a)


class D:
    def __init__(self, a, b):
        pass

    def __new__(cls, *args):
        pass


def make(a, b):
    D(b, a)
