"""W0239 overridden-final-method: an override of a method decorated with typing.final."""
import typing
import typing as t
import typing_extensions
from typing import final
from typing import final as sealed


class Base:
    @final
    def plain(self):
        return 1

    @typing.final
    def attr(self):
        return 1

    @t.final
    def aliased(self):
        return 1

    @sealed
    def renamed(self):
        return 1

    @typing_extensions.final
    def extension(self):
        return 1

    @final
    @property
    def prop(self):
        return 1

    @final
    @classmethod
    def klass(cls):
        return 1

    @final
    @staticmethod
    def static():
        return 1

    @final
    async def task(self):
        return 1

    def open(self):
        return 1

    @final
    def _private(self):
        return 1

    @final
    def __dunder__(self):
        return 1


class Child(Base):
    def plain(self):  # positive
        return 2

    def attr(self):  # positive
        return 2

    def aliased(self):  # positive
        return 2

    def renamed(self):  # positive
        return 2

    def extension(self):  # positive?
        return 2

    @property
    def prop(self):  # positive?
        return 2

    @classmethod
    def klass(cls):  # positive
        return 2

    @staticmethod
    def static():  # positive
        return 2

    async def task(self):  # positive
        return 2

    def open(self):  # negative
        return 2

    def _private(self):  # positive
        return 2

    def __dunder__(self):  # positive
        return 2

    def new(self):  # negative
        return 2


class GrandChild(Child):
    def plain(self):  # positive: the final method is two levels up
        return 3

    def open(self):  # negative
        return 3


class Other:
    def plain(self):  # negative: Other has no final ancestor
        return 4


class Unknown(unknown_base):
    def plain(self):  # negative
        return 5


class Rebound(Base):
    plain = None  # negative: not a def

    def attr(self, extra):  # positive, and the signature differs as well
        return extra
