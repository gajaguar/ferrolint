import abc
import collections
import enum
import typing
from abc import ABC
from collections import OrderedDict
from collections.abc import Mapping
from typing import Generic, NamedTuple, Protocol, TypeVar

T = TypeVar("T")


class A:
    pass


class B(A):
    pass


class C(B):
    pass


class D(C):
    pass


class Diamond1(A):
    pass


class Diamond2(A):
    pass


class Diamond(Diamond1, Diamond2):
    pass


class Multi(A, B, Diamond1):
    pass


class Deep(D, Diamond):
    pass


class Err(ValueError):
    pass


class Err2(Err):
    pass


class Ofile(FileNotFoundError):
    pass


class Abc(ABC):
    pass


class Abc2(abc.ABC):
    pass


class Gen(Generic[T]):
    pass


class Proto(Protocol):
    pass


class GenB(B, Generic[T]):
    pass


class FromDict(dict):
    pass


class FromDict2(FromDict, B):
    pass


class FromOrdered(OrderedDict):
    pass


class FromMapping(Mapping):
    pass


class FromMapping2(FromMapping, B):
    pass


class FromNamed(NamedTuple):
    x: int


class FromEnum(enum.Enum):
    A = 1


class FromEnum2(FromEnum):
    pass


class FromInt(int):
    pass


class FromBool(bool):
    pass


class FromStr(str, B):
    pass


class FromFrozen(frozenset):
    pass


class FromTuple(tuple):
    pass


class FromType(type):
    pass


class FromObject(object):
    pass


class Unknown(Missing):
    pass


class Unknown2(Unknown, B):
    pass


class Chain(typing.Generic[T], B):
    pass


class FromDefault(collections.defaultdict):
    pass


class ExcGroup(ExceptionGroup):
    pass


class Base(BaseException):
    pass


import os
import select
import socket


class FromIoError(IOError):
    pass


class FromEnvironmentError(EnvironmentError):
    pass


class FromOsError(os.error):
    pass


class FromSocketError(socket.error):
    pass


class FromSelectError(select.error):
    pass
