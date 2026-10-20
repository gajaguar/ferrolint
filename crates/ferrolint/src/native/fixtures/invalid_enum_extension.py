"""E0244 invalid-enum-extension: an enumeration with members used as a base."""
import enum
from enum import Enum, Flag, IntEnum, auto


class Color(Enum):
    RED = 1
    GREEN = 2


class Empty(Enum):
    def describe(self):
        return self.name


class Annotated(Enum):
    kind: int


class AnnotatedValue(Enum):
    kind: int = 1


class Ignored(Enum):
    _ignore_ = ["a"]


class Helper(Enum):
    _order_ = "A B"


class Auto(Enum):
    FIRST = auto()


class Tuple(Enum):
    A, B = 1, 2


class Conditional(Enum):
    if enum:
        MEMBER = 1


class Nested(Enum):
    class Meta:
        pass


class Function(Enum):
    def value(self):
        return 1

    value = 2


class Number(IntEnum):
    ONE = 1


class Bits(Flag):
    X = auto()


class Plain:
    X = 1


class FromColor(Color):  # positive
    pass


class FromEmpty(Empty):  # negative
    pass


class FromAnnotated(Annotated):  # negative
    pass


class FromAnnotatedValue(AnnotatedValue):  # positive
    pass


class FromIgnored(Ignored):  # negative
    pass


class FromHelper(Helper):  # positive: the helper assignment counts
    pass


class FromAuto(Auto):  # positive
    pass


class FromTuple(Tuple):  # positive
    pass


class FromConditional(Conditional):  # positive
    pass


class FromNested(Nested):  # negative
    pass


class FromFunction(Function):  # negative: the name is also a def
    pass


class FromNumber(Number):  # positive
    pass


class FromBits(Bits, Flag):  # positive
    pass


class Mixed(Color, str):  # positive
    pass


class FromPlain(Plain):  # negative: not an enumeration
    pass


class FromEnum(Enum):  # negative
    pass


class FromIntEnum(IntEnum):  # negative
    pass


class Attribute(enum.Enum):
    pass


class Deep(FromColor):  # negative: FromColor has no members of its own
    pass


class Two(Color, Number):  # positive twice, one message per base
    pass


class Plain2(Plain, Empty):  # negative
    pass
