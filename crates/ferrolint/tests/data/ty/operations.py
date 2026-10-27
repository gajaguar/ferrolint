"""Operation rules: ty answers, and only a single concrete operand type counts."""
from typing import Annotated, TypeVar

T = TypeVar("T")
Alias = Annotated[int, 1]


class Plain:
    pass


class Generic:
    def __class_getitem__(cls, item):
        return cls


def reported():
    number = 1
    text = "a"
    print(number[0])
    print(Plain()[0])
    number[0] = 1
    del number[0]
    number()
    for _ in number:
        pass
    print([item for item in number])
    first, second = number
    print(1 in number)
    print(2 not in number)
    print(-text)
    print(number + None)  # E1131: only ty reports it
    number += None  # E1131: only ty reports it
    return first, second


def silent(maybe: int | None, either: int | str, anything: object, var: T, unknown):
    print(maybe[0], either[0], anything[0], var[0], unknown[0])
    maybe[0] = 1
    del either[0]
    either()
    anything()
    for _ in maybe:
        pass
    first, second = either
    print(1 in maybe, -either, either + None, Generic[int])
    print(Alias(1), first, second)
    d = {}
    d[1] = 2
    print(1 < Plain())
