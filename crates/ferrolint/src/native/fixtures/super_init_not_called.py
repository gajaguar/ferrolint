import typing


class A:
    def __init__(self):
        self.a = 1


class B(A):
    def __init__(self):
        self.b = 1


class C(A):
    def __init__(self):
        super().__init__()


class D(A):
    def __init__(self):
        A.__init__(self)


class Abstract:
    def __init__(self):
        raise NotImplementedError


class Empty:
    def __init__(self):
        """Nothing."""


class Passing:
    def __init__(self):
        pass


class E(Abstract, Empty, Passing):
    def __init__(self):
        self.e = 1


class F(B):
    def __init__(self):
        self.f = 1


class G(Exception):
    def __init__(self):
        self.g = 1


class Inherits(A):
    pass


class H(A, Inherits):
    def __init__(self):
        A.__init__(self)


class Proto(typing.Protocol):
    def __init__(self):
        self.p = 1


class I(Proto):
    def __init__(self):
        self.i = 1


class Nested(A):
    def __init__(self):
        def later():
            super(Nested, self).__init__()

        later()


class Other:
    def __init__(self):
        self.o = 1


class J(A, Other):
    def __init__(self):
        Other.__init__(self)
