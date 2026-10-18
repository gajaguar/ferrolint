class A:
    __slots__ = 1


class B:
    __slots__ = None


class C:
    __slots__ = b"ab"


class D:
    __slots__ = lambda: 1


class E:
    __slots__ = "ab"
    x = 1


class F:
    __slots__ = ("a", 1, "", None, b"x", 2.5, True, ..., "ok")


class G:
    x = 1
    __slots__ = ("x", "y", "z")

    def y(self):
        pass

    @property
    def z(self):
        pass


class H:
    q: int
    __slots__ = ("q",)


class I:
    q: int = 1
    __slots__ = ("q",)


class J:
    __slots__ = {"a": 1, "b": 2, 3: 4}
