class Slotted:
    __slots__ = ["a", "b"]

    def __init__(self):
        self.a = 1
        self.b = 2
        self.c = 3

    def later(self):
        self.d = 4
        print(self.c)

    def before(self):
        print(self.k)
        self.k = 1


class Mixed:
    __slots__ = ("a", "__dict__")

    def __init__(self):
        self.a = 1
        self.z = 2
        self.y = 3


class Inherits(Slotted):
    def __init__(self):
        super().__init__()
        self.e = 5
        self.f = 6


class Chain(Slotted):
    __slots__ = ("x",)

    def __init__(self):
        super().__init__()
        self.x = 1
        self.a = 2
        self.w = 3


class Empty:
    __slots__ = ()

    def __init__(self):
        self.q = 1
        self.r = 1


class Single:
    __slots__ = "a"

    def __init__(self):
        self.a = 1
        self.q = 1
        self.r = 1


class Dicted:
    __slots__ = {"a": "doc", "b": "doc"}

    def __init__(self):
        self.a = 1
        self.q = 1
        self.r = 1


class Named:
    __slots__ = ("m", "n")


named = Named()
named.m = 1
named.n = 2
named.o = 3
named.p = 4
