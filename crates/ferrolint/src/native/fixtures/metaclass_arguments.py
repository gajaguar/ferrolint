import abc
import enum
from abc import ABCMeta


class Meta(type):
    def a(self):
        pass

    def b(mcs):
        pass

    @classmethod
    def c(cls):
        pass

    @classmethod
    def d(mcs):
        pass

    def __new__(cls):
        pass

    def __new__(mcs, name):
        pass

    def e(*args):
        pass

    def g(self, /):
        pass

    def h(cls, /):
        pass

    @staticmethod
    def i(self):
        pass

    @property
    def j(self):
        pass

    def __init_subclass__(cls):
        pass


class Sub(Meta):
    def f(self):
        pass


class Abc(abc.ABCMeta):
    def g(self):
        pass


class Abc2(ABCMeta):
    def g(self):
        pass


class En(enum.EnumType):
    def g(self):
        pass


class Plain(object):
    def h(this):
        pass
