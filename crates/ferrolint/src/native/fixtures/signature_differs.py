from typing import overload


class A:
    def m1(self, a, b=1, c=2):
        return a, b, c

    def m2(self, a, b=1):
        return a, b

    def m3(self, a, b=1):
        return a, b

    def m4(self, a, b=1):
        return a, b

    def m5(self, a, *, k=1):
        return a, k

    def _m6(self, a, b=1):
        return a, b

    def __m7(self, a, b=1):
        return a, b

    def m8(self, a, b=1):
        return a, b

    @classmethod
    def m9(cls, a, b=1):
        return a, b

    @staticmethod
    def m10(a, b=1):
        return a, b

    def __post_init__(self, a=1):
        return a

    def __new__(cls, a=1):
        return a

    def m11(self, a, b=1):
        return a, b

    def m12(self, a=1, b=2):
        return a, b

    def __eq__(self, other=None):
        return other

    @property
    def p(self):
        return 1

    @p.setter
    def p(self, value=1):
        return value

    async def m13(self, a, b=1):
        return a, b


class B(A):
    def m1(self, a, b=1, c=2):
        return a, b, c

    def m2(self, a, b):
        return a, b

    def m3(self, a, *args):
        return a, args

    def m4(self, a, **kwargs):
        return a, kwargs

    def m5(self, a, *, k):
        return a, k

    def _m6(self, a, b):
        return a, b

    def __m7(self, a, b):
        return a, b

    @overload
    def m8(self, a, b):
        ...

    @overload
    def m8(self, a):
        ...

    def m8(self, a, b=1):
        return a, b

    @classmethod
    def m9(cls, a, b):
        return a, b

    @staticmethod
    def m10(a, b):
        return a, b

    def __post_init__(self, a):
        return a

    def __new__(cls, a):
        return a

    def m11(self, a, b=3):
        return a, b

    def m12(self, a, b):
        return a, b

    def __eq__(self, other):
        return other

    @property
    def p(self):
        return 2

    @p.setter
    def p(self, value):
        return value

    async def m13(self, a, b):
        return a, b


class C(B):
    def m2(self, a, b):
        return a, b

    def m12(self, a=1):
        return a


class D(A):
    @overload
    def m2(self, a, b=1):
        ...

    @overload
    def m2(self, a):
        ...

    def m2(self, a, b):
        return a, b

    def m1(self, a, b=1, c=2, *args):
        return a

    def m3(self, a, b, c=1):
        return a
