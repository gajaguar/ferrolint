import enum
import typing
from dataclasses import dataclass
from typing import NamedTuple, TypedDict


class Empty:
    pass


class One:
    def run(self):
        pass


class Two:
    def run(self):
        pass

    def stop(self):
        pass


class Private:
    def _run(self):
        pass

    def __str__(self):
        return ""

    def __repr__(self):
        return ""


class WithInit:
    def __init__(self):
        pass


class Attrs:
    value = 1
    other = 2


class Base:
    def one(self):
        pass


class Child(Base):
    def two(self):
        pass


class Overrides(Base):
    def one(self):
        pass


class Grand(Child):
    pass


class Shadow(Base):
    one = 1


class MyError(Exception):
    pass


class Oops(ValueError):
    pass


class Custom(MyError):
    pass


class LooksLikeException:
    pass


class BadException:
    pass


class Meta(type):
    pass


class Color(enum.Enum):
    RED = 1


class Flag(enum.IntEnum):
    ON = 1


class Point(NamedTuple):
    x: int


class Doc(TypedDict):
    key: str


class Doc2(typing.TypedDict):
    key: str


@dataclass
class Data:
    x: int


@dataclass(frozen=True)
class Frozen:
    x: int


class WithObject(object):
    pass


class Generic(typing.Generic[typing.TypeVar("T")]):
    pass


class Nested:
    class Inner:
        def go(self):
            pass

    def outer(self):
        pass


class Excluded:
    pass


class UnderExcluded(Excluded):
    pass


class FromBuiltin(dict):
    pass


class FromList(list):
    def extra(self):
        pass


class Special:
    def __len__(self):
        return 0

    def __init__(self):
        pass


class Lowercase:
    def __abc__(self):
        pass

    def __Bad__(self):
        pass

    def __a1__(self):
        pass
