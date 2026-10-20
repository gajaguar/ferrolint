import abc
import functools
from functools import cached_property
from abc import abstractproperty


class lazy(property):
    pass


class Base:
    @property
    def p1(self):
        return 1

    @property
    def p2(self):
        return 2

    @property
    def p3(self):
        return 3

    @property
    def p4(self):
        return 4

    def m1(self):
        return 1

    def m2(self):
        return 2

    def m3(self):
        return 3

    def m4(self):
        return 4

    async def a1(self):
        return 1

    def a2(self):
        return 1

    async def a3(self):
        return 1

    @functools.cached_property
    def c1(self):
        return 1

    @abstractproperty
    def ab1(self):
        return 1

    @lazy
    def l1(self):
        return 1

    @property
    def s1(self):
        return 1

    @s1.setter
    def s1(self, value):
        pass

    @property
    def s2(self):
        return 1

    @s2.setter
    def s2(self, value):
        pass

    @staticmethod
    def st1():
        return 1

    @classmethod
    def cm1(cls):
        return 1

    def __private(self):
        return 1

    @property
    def __pp(self):
        return 1


class Child(Base):
    def p1(self):
        return 1

    @classmethod
    def p2(cls):
        return 2

    @staticmethod
    def p3():
        return 3

    @property
    def p4(self):
        return 4

    @property
    def m1(self):
        return 1

    @cached_property
    def m2(self):
        return 2

    @functools.cached_property
    def m3(self):
        return 3

    @lazy
    def m4(self):
        return 4

    def a1(self):
        return 1

    async def a2(self):
        return 1

    async def a3(self):
        return 1

    def c1(self):
        return 1

    def ab1(self):
        return 1

    def l1(self):
        return 1

    @s1.setter
    def s1(self, value):
        pass

    def s2(self, value):
        pass

    @property
    def st1(self):
        return 1

    @property
    def cm1(self):
        return 1

    @property
    def _Base__private(self):
        return 1

    @property
    def __pp(self):
        return 1


class GrandChild(Child):
    def p4(self):
        return 4

    @property
    def m1(self):
        return 1

    async def a1(self):
        return 1


class Unrelated:
    x = 1

    @property
    def m1(self):
        return 1


class Mixed(Unrelated):
    def m1(self):
        return 1

    async def x(self):
        return 1


class Both(Base):
    @property
    async def a1(self):
        return 1

    @staticmethod
    def m1():
        return 1

    def __init__(self):
        pass
