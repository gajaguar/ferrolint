import typing
from typing import final

name = "x"
greeting = f"hello {name}"
joined = "a" f"b{name}"


@final
class A:
    pass


@typing.final
class B:
    pass


def posonly(a, /, b):
    return a + b


plain = lambda a, /: a

if (n := len(name)) > 1:
    pass

try:
    pass
except* ValueError:
    pass

try:
    pass
except ExceptionGroup:
    pass

raise ExceptionGroup("x", [ValueError()])

type Alias = int


class C[T]:
    pass


def generic[T, *Ts, **P](a: T) -> T:
    return a


lists = [*x for x in [[1]]]
