from typing import ClassVar
import typing


class J:
    __slots__ = ["a"]
    b: int
    c: ClassVar[int]
    d: "ClassVar[int]"
    e: typing.ClassVar[int]
    f: int = 3
    g = 3
    a: int


class K:
    __slots__ = ("__dict__", "a")
    b: int


class L(J):
    __slots__ = ("x",)
    b: int


class M:
    __slots__ = ()
    b: int


class N(object):
    __slots__ = ("a",)
    b: int


class O(K):
    __slots__ = ("x",)
    b: int


class P:
    __slots__ = ("a",)

    if True:
        b: int
    c: int


class Q:
    __slots__ = [x for x in "ab"]
    b: int


class R(Q):
    __slots__ = ("a",)
    b: int


class S:
    __slots__ = ("a", "b")
    __slots__ = ("c",)
    a: int


class T(M):
    __slots__ = ("x",)
    b: int
