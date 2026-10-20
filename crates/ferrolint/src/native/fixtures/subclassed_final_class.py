"""W0240 subclassed-final-class: a base decorated with typing.final."""
import typing
import typing as t
import typing_extensions
from typing import Generic, TypeVar, final
from typing import final as sealed

T = TypeVar("T")


@final
class Sealed:
    pass


@typing.final
class Attr:
    pass


@t.final
class Aliased:
    pass


@sealed
class Renamed:
    pass


@typing_extensions.final
class Extension:
    pass


@final()
class Called:
    pass


@final
class SealedGeneric(Generic[T]):
    pass


class Open:
    pass


class FromSealed(Sealed):  # positive
    pass


class FromAttr(Attr):  # positive
    pass


class FromAliased(Aliased):  # positive
    pass


class FromRenamed(Renamed):  # positive
    pass


class FromExtension(Extension):  # positive?
    pass


class FromCalled(Called):  # positive?
    pass


class FromSubscript(SealedGeneric[int]):  # positive?
    pass


class Two(Open, Sealed, Attr):  # positive twice: one message per final base
    pass


class Nested(FromSealed):  # negative: FromSealed is not final itself
    pass


class FromOpen(Open):  # negative
    pass


class Unknown(unknown_base):  # negative
    pass


def local():
    @final
    class Inner:
        pass

    class Child(Inner):  # positive
        pass

    return Child


@final
class WithMeta(metaclass=type):
    pass


class Keyword(Open, metaclass=type):  # negative
    pass


class FromMeta(WithMeta):  # positive
    pass
