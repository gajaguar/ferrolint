import os
from typing import _SpecialForm
from somewhere import Unknown, deco


def func():
    pass


def raises():
    raise ValueError


def returns():
    return 1


@deco
def decorated():
    pass


@Unknown.deco
def decorated2():
    pass


@property
def prop():
    pass


async def afunc():
    pass


lam = lambda: 1
number = 1
text = "x"


def gen():
    yield 1


class A:
    def method(self):
        pass

    @property
    def prop(self):
        return 1

    @staticmethod
    def static():
        pass

    @classmethod
    def klass(cls):
        pass

    def overwritten(self):
        pass

    def __init__(self):
        self.overwritten = 1
        self.other = 2

    def other(self):
        pass

    def check(self, param, default=None):
        if self.method == 1:
            pass
        if self.method == param:
            pass
        if self.method == self.method:
            pass
        if self.prop == 1:
            pass
        if self.static == 1:
            pass
        if self.klass == 1:
            pass
        if self.overwritten == 1:
            pass
        if self.other == 1:
            pass
        if A.method == 1:
            pass
        if A.static == 1:
            pass
        if A.klass == 1:
            pass
        if 1 == self.method:
            pass
        if param == self.method:
            pass
        if self.method != 1:
            pass
        if self.method < 1:
            pass
        if self.method is None:
            pass
        if self.method in [1]:
            pass
        if self.method == 1 == 1:
            pass
        if 1 == 1 == self.method:
            pass

    def lambdas(self):
        if (lambda: 1) == 1:
            pass


def top(param, default=None, other=func):
    if func == 1:
        pass
    if func == param:
        pass
    if func == default:
        pass
    if func == other:
        pass
    if func == func:
        pass
    if func == raises:
        pass
    if raises == 1:
        pass
    if returns == 1:
        pass
    if returns == raises:
        pass
    if decorated == 1:
        pass
    if decorated2 == 1:
        pass
    if prop == 1:
        pass
    if afunc == 1:
        pass
    if lam == 1:
        pass
    if gen == 1:
        pass
    if len == 1:
        pass
    if print == "x":
        pass
    if func == number:
        pass
    if func == text:
        pass
    if func == None:
        pass
    if func == os.getcwd:
        pass
    if func == A:
        pass
    if func == A():
        pass
    if func == [1]:
        pass
    if func == -1:
        pass
    if func == f"{param}":
        pass
    if func.__name__ == 1:
        pass
    if func() == 1:
        pass
    if func is None:
        pass
    if func > 1:
        pass
    if func >= 1:
        pass
    if func <= 1:
        pass
    if func < 1:
        pass
    x = func == 1
    y = [func == 1]
    z = (func == 1) == 2
    return x, y, z
