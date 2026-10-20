"""E0110 abstract-class-instantiated: calling a class that has abstract methods."""
import abc
import typing
from abc import ABC, ABCMeta, abstractmethod


class Base(ABC):
    @abstractmethod
    def run(self):
        pass


class ByMeta(metaclass=ABCMeta):
    @abstractmethod
    def run(self):
        pass


class ByAbcMeta(metaclass=abc.ABCMeta):
    @abc.abstractmethod
    def run(self):
        pass


class NoMeta:
    @abstractmethod
    def run(self):
        pass


class Custom(ABCMeta):
    pass


class ByCustom(metaclass=Custom):
    @abstractmethod
    def run(self):
        pass


class Concrete(Base):
    def run(self):
        return 1


class Partial(Base):
    pass


class Deep(Partial):
    pass


class Property(ABC):
    @property
    @abstractmethod
    def value(self):
        pass


class OldProperty(ABC):
    @abc.abstractproperty
    def value(self):
        pass


class ClassMethod(ABC):
    @classmethod
    @abc.abstractclassmethod
    def make(cls):
        pass


class Raises(ABC):
    def run(self):
        raise NotImplementedError


class ProtocolLike(typing.Protocol):
    @abstractmethod
    def run(self):
        pass


class Mixin:
    def run(self):
        return 2


class MixedFirst(Mixin, Base):
    pass


class MixedLast(Base, Mixin):
    pass


class AbstractAgain(Concrete):
    @abstractmethod
    def run(self):
        pass


class Empty(ABC):
    pass


class Rebound(Base):
    run = None


class Factory(Base):
    @classmethod
    def build(cls):
        return Factory()  # negative: instantiated in its own body

    def other(self):
        return Base()  # positive: another class

    def nested(self):
        def inner():
            return Factory()  # negative: still inside Factory

        return inner

    attr = Base()  # positive: in a class body


def module_level():
    Base()  # positive
    ByMeta()  # positive
    ByAbcMeta()  # positive
    NoMeta()  # negative: no abstract metaclass
    ByCustom()  # negative: the metaclass is not ABCMeta itself
    Concrete()  # negative
    Partial()  # positive
    Deep()  # positive
    Property()  # positive
    OldProperty()  # positive
    ClassMethod()  # positive
    Raises()  # negative: only decorated methods count
    ProtocolLike()  # negative
    MixedFirst()  # negative: Mixin.run comes first
    MixedLast()  # positive
    AbstractAgain()  # positive
    Empty()  # negative
    Rebound()  # negative
    Base.run  # negative: no call
    x = [Base() for _ in range(2)]  # positive
    return x


def with_args():
    return Base(
        1,
        2,
    )  # positive, at the first line


def unknown():
    return unknown_name()  # negative


class Child(Base):
    def make(self):
        return Base()  # positive: Base is not the enclosing class

    def clone(self):
        return Child()  # negative
