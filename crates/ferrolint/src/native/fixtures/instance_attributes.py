from dataclasses import InitVar, dataclass
from typing import ClassVar


class A:
    x = 0

    def __init__(self):
        self.a = 1
        self.helper()

    def helper(self):
        self.h = 1

    def other(self):
        self.o = 1
        self.x = 2
        self.a = 3
        self.n += 1
        self.t: int
        for self.f in range(3):
            pass

        def inner():
            self.i = 1

        inner()

    @property
    def p(self):
        return 1

    @p.setter
    def p(self, value):
        self.ps = value

    def set_q(self, value):
        self.q = value

    q = property(None, set_q)

    def setup(self):
        setattr(self, "sa", 1)


class AMixin:
    def go(self):
        self.m = 1


class B(A):
    def go(self):
        self.a = 2
        self.h = 2
        self.b = 1


class C:
    def __init__(self):
        self.go = 1

    def go(self):
        return 1


class D(C):
    def go(self):
        return 2


class E:
    def __init__(self):
        print(self.e)
        self.e = 1
        self.g()

    def g(self):
        return self.e

    def k(self):
        print(self.z)
        self.z = 1


class F:
    def __init__(self):
        self.a1 = self.a2 = self.a3 = self.a4 = 1
        self.a5, self.a6 = 1, 2
        self.a7 = 1

    def m(self):
        self.a8 = 1
        self.a8 = 2


class G:
    @staticmethod
    def s(self):
        self.st = 1

    @classmethod
    def c(cls):
        cls.cl = 1

    def __new__(cls):
        cls.nw = 1


class Hider:
    def __init__(self):
        self.prop = 1
        self.static = 1
        self.__private = 1

    @property
    def prop(self):
        return 1

    @staticmethod
    def static():
        return 1


class HiderChild(Hider):
    def __private(self):
        return 1


def factory_hook(func):
    return func


class Factory:
    @factory_hook
    def hook(obj, create):
        obj.hook = create

    def lambdas(self):
        return lambda: setattr(self, "late", 1)


class Exclusive:
    def __init__(self):
        try:
            print(self.value)
        except AttributeError:
            self.value = 1

    def run(self):
        try:
            self.other = 1
        except ValueError:
            print(self.other)
        print(self.after)
        self.after = 1
        print(self.after)


class Called:
    def __init__(self):
        self.prepare()
        self.ready = 1

    def prepare(self):
        return self.ready


class Many:
    def __init__(self):
        self.a = self.b = self.c = self.d = 1

    def more(self):
        self.e = self.f = self.g = 1
        self.h += 1


class Shadowed:
    def __init__(self):
        pass

    def run(self):
        def inner(self):
            self.inner = 1

        return inner


class Annotated:
    value: int

    def set(self):
        self.value = 1
        self.__dict__ = {}


class Client:
    def __init__(self):
        self.known = 1


def make_client_class():
    class Made(Client):
        pass

    return Made


def use_clients():
    client = Client()
    client.known = 2
    client.extra = 1
    made_class = make_client_class()
    made = made_class()
    made.other = 1
    again = Client()
    again = None
    again.never = 1


class ClientTest:
    def setUp(self):
        client = Client()
        client.ready = 1


@dataclass
class Fields:
    a: int
    b: int
    c: int
    d: int
    e: int
    f: int
    g: ClassVar[int] = 0
    h: InitVar[int] = 0
    i: int = 0

    def __post_init__(self, h):
        self.j = h


class Factory:
    def __init__(self):
        self.known = 1

    @classmethod
    def build(cls):
        made = cls()
        made.extra = 1
        return made

    def not_a_classmethod(cls):
        made = cls()
        made.skipped = 1
        return made
