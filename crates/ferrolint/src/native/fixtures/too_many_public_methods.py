class A:
    def a(self):
        pass

    def b(self):
        pass

    def c(self):
        pass

    def _p(self):
        pass

    x = 1

    def x(self):
        pass


class B:
    @property
    def a(self):
        pass

    @a.setter
    def a(self, v):
        pass

    def b(self):
        pass

    async def c(self):
        pass


class C:
    def a(self):
        pass

    def a(self):
        pass

    if True:
        def b(self):
            pass
    else:
        def b(self):
            pass

    def d(self):
        def e(self):
            pass

    class Inner:
        def f(self):
            pass


import enum
from django.db import models


class EnumEmpty(enum.Enum):
    X = 1


class EnumOne(enum.Enum):
    X = 1

    def a(self):
        pass


class EnumTwo(enum.StrEnum):
    X = "x"

    def a(self):
        pass

    def b(self):
        pass


class EnumMixin(str, enum.Enum):
    X = "x"

    def a(self):
        pass

    def b(self):
        pass


class Choices(models.TextChoices):
    X = "x"

    def a(self):
        pass

    def b(self):
        pass
