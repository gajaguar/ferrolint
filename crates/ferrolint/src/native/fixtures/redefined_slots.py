class A:
    __slots__ = ("a", "b")


class B(A):
    __slots__ = ("b", "c")


class C(B):
    __slots__ = ("a", "c", "d")


class Fine(A):
    __slots__ = ("x", "y")


class NoSlotsMiddle(A):
    pass


class BelowMiddle(NoSlotsMiddle):
    __slots__ = ("a",)


class Plain:
    pass


class Broken(Plain):
    __slots__ = ("h",)


class UnderBroken(Broken):
    __slots__ = ("h",)


class Left:
    __slots__ = ["left", "shared"]


class Right:
    __slots__ = ["right", "shared"]


class Both(Left, Right):
    __slots__ = ["shared", "own", "left"]


class AsSet(A):
    __slots__ = {"a"}


class AsDict(A):
    __slots__ = {"a": "doc a", "other": "doc"}


class Spread(A):
    __slots__ = (
        "a",
        "z",
        "b",
    )


class Repeated(A):
    __slots__ = ("a", "a")


class Mixed(A):
    __slots__ = ("a", 1, name)


class FromBuiltin(Exception):
    __slots__ = ("e",)


class UnderBuiltin(FromBuiltin):
    __slots__ = ("e",)


class Empty(A):
    __slots__ = ()


class AString(A):
    __slots__ = "a"


class Reassigned(A):
    __slots__ = ("a",)
    __slots__ = ("z",)


class NoOwnSlots(A):
    attribute = 1


class Unknown(Missing):
    __slots__ = ("a",)
