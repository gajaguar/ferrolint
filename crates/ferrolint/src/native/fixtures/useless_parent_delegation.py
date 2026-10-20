from typing import Any, Optional


class Base:
    def m1(self, a, b):
        return a, b

    def m2(self, a, b=1):
        return a, b

    def m3(self, a, *args, **kwargs):
        return a

    def m4(self, a, *, k):
        return a, k

    def m5(self, a: int, b: str) -> int:
        return a

    def m6(self, a, b=None):
        return a, b

    def m7(self, a=(), b=[], c={}):
        return a, b, c

    def m8(self, a=1):
        return a

    def m9(self, *args):
        return args

    def m10(self, a):
        return a

    def m11(self, a, b):
        return a, b

    def m12(self, a):
        return a

    def m13(self, a):
        return a

    @property
    def p(self):
        return 1

    m14 = m10

    def __eq__(self, other):
        return True

    def __hash__(self):
        return 1

    async def m15(self):
        return 1

    @classmethod
    def m16(cls):
        return 1

    @staticmethod
    def m17():
        return 1

    def m18(self, a):
        return a

    def m19(self, a, b):
        return a, b


class Child(Base):
    def m1(self, a, b):
        return super().m1(a, b)

    def m2(self, a, b=1):
        super().m2(a, b)

    def m3(self, a, *args, **kwargs):
        return super().m3(a, *args, **kwargs)

    def m4(self, a, *, k):
        return super().m4(a, k=k)

    def m5(self, a: int, b: str) -> int:
        return super().m5(a, b)

    def m6(self, a, b=1):
        return super().m6(a, b)

    def m7(self, a=(), b=[], c={}):
        return super().m7(a, b, c)

    def m8(self, a=2):
        return super().m8(a)

    def m9(self, *args):
        return super().m9(*args)

    def m10(self, a):
        return super(Child, self).m10(a)

    def m11(self, a, b):
        return super().m11(b, a)

    def m12(self, a):
        """Docstring."""
        return super().m12(a)

    def m13(self, a):
        return super().m12(a)

    @property
    def p(self):
        return super().p

    def m14(self, a):
        return super().m14(a)

    def __eq__(self, other):
        return super().__eq__(other)

    def __hash__(self):
        return super().__hash__()

    async def m15(self):
        return super().m15()

    @classmethod
    def m16(cls):
        return super().m16()

    @staticmethod
    def m17():
        return super().m17()

    def m18(self, a):
        return super().m18(a + 1)

    def m19(self, a, b):
        return super().m19(a, b=b)


class Annotated(Base):
    def m5(self, a: float, b: str) -> int:
        return super().m5(a, b)

    def m1(self, a: int, b):
        return super().m1(a, b)

    def m10(self, a: Any) -> Optional[int]:
        return super().m10(a)

    def m11(self, a, b) -> int:
        return super().m11(a, b)

    def m18(self, a: int):
        return super().m18(a)


class Variadic(Base):
    def m3(self, a, b, *args, **kwargs):
        return super().m3(a, b, *args, **kwargs)

    def m9(self, a, *args):
        return super().m9(a, *args)

    def m1(self, a, b, *args):
        return super().m1(a, b, *args)

    def m10(self, **kwargs):
        return super().m10(**kwargs)

    def m2(self, a, b=1, **kwargs):
        return super().m2(a, b, **kwargs)

    def m4(self, a, *, k, **kwargs):
        return super().m4(a, k=k)

    def m12(self, a, **kwargs):
        return super().m12(a, **kwargs)


class Plain:
    def f(self, a):
        return super().f(a)

    def g(self):
        super().g()

    def __repr__(self):
        return super().__repr__()

    def __init__(self):
        super().__init__()

    def __new__(cls):
        return super().__new__(cls)

    def h(self, a, *, k=None):
        return super().h(a, k=k)

    def i(self, a, b):
        x = super().i(a, b)
        return x

    def j(self, a):
        return super().j(a=a)

    def k(self, self2, a):
        return super().k(a)


class Lone:
    def __eq__(self, other):
        return super().__eq__(other)

    def __hash__(self):
        return super().__hash__()


class Eq:
    def __eq__(self, other):
        return super().__eq__(other)

    def __hash__(self):
        return super().__hash__()


class Nested(Base):
    if True:

        def m1(self, a, b):
            return super().m1(a, b)

    def outer(self):
        def m1(a, b):
            return super().m1(a, b)

        return m1


class Other(Plain):
    def f(self, a):
        return super().f(a)

    def g(self):
        super().g()

    def zz(self):
        return super(Other, self).zz()

    def yy(self):
        return super(Plain, self).yy()

    def xx(self):
        return super(self.__class__, self).xx()

    def ww(self):
        return super(Other).ww()
