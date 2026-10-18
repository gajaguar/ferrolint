import abc
from abc import ABC, ABCMeta, abstractmethod
from typing import Generic, Protocol, TypeVar

T = TypeVar("T")


class Base:
    def run(self):
        raise NotImplementedError

    def stop(self):
        """Documented first."""
        raise NotImplementedError("stop")

    def pause(self):
        pass

    def concrete(self):
        return 1


class Child(Base):
    pass


class Overrides(Base):
    def run(self):
        return 1

    stop = None


class Grandchild(Child):
    def stop(self):
        return 2


class StillAbstract(Base):
    def run(self):
        raise NotImplementedError


class Decorated:
    @abstractmethod
    def first(self):
        return 1

    @abc.abstractproperty
    def second(self):
        return 2


class FromDecorated(Decorated):
    pass


class WithMeta(Base, metaclass=ABCMeta):
    pass


class WithAbc(Base, ABC):
    pass


class Proto(Base, Protocol):
    pass


class Generics(Base, Generic[T]):
    run = stop = 1


class Mixin:
    def run(self):
        return 3


class Diamond(Mixin, Base):
    def stop(self):
        return 4


class Other(Base):
    def run(self):
        x = 1
        raise NotImplementedError

    def stop(self):
        raise ValueError(NotImplementedError)


class Leaf(Other):
    pass


def factory():
    class Inner(Base):
        def run(self):
            return 5

    return Inner


class Duplicated(Base, Base):
    pass
