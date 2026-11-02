import abc
from factory import post_generation


class A:
    def a(obj):
        pass

    def b(*args):
        pass

    def c(this, x):
        pass

    def d(self):
        pass

    def e(_self):
        pass

    def f(cls):
        pass

    def g(*, k):
        pass

    @staticmethod
    def h(obj):
        pass

    @classmethod
    def i(obj):
        pass

    def __new__(obj):
        pass

    def __init_subclass__(obj):
        pass

    def __class_getitem__(obj, item):
        pass

    @post_generation
    def j(obj, create, extracted):
        pass

    @property
    def k(obj):
        pass

    def m(obj, /):
        pass

    def n(self, /, x):
        pass

    @abc.abstractmethod
    def o(obj):
        pass

    if True:
        def p(obj):
            pass

    def q(self):
        class Local:
            def r(obj):
                pass


class Meta(type):
    def s(obj):
        pass

    def t(cls):
        pass


def func(obj):
    pass
