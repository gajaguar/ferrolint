import dataclasses
import threading
import unittest
from dataclasses import dataclass, field
from threading import Thread as T


class A:
    def f(self, x):
        self.__name__ = 0
        self.__name__ = None
        self.__name__ = []
        self.__name__ = ()
        self.__name__ = {}
        self.__name__ = False
        self.__name__ = [1]
        self.__name__ = "a"
        self.__name__ = x
        self.__name__ = lambda: 1
        self.__name__ = b"a"
        self.__name__ = 1.5
        self.__name__ = f"{x}"
        self.__name__ = ...
        self.__name__ = 1, 2
        self.__name__ = self.__name__ = 5
        a = self.__name__ = 5
        a, self.__name__ = 5, 6
        self.__name__ += 1


def thr():
    threading.Thread()
    threading.Thread(None)
    threading.Thread(None, print)
    threading.Thread(target=print)
    threading.Thread(**{})
    threading.Thread(*[1])
    T()
    T(group=None)
    T(target=None, name="x")


class TC(unittest.TestCase):
    def test(self):
        self.assertTrue(1)
        self.assertTrue(True, "msg")
        self.assertFalse(0)
        self.assertFalse(None)
        self.assertTrue("a")
        self.assertTrue(x)
        self.assertEqual(1, 1)
        self.assertEqual(1, x)
        self.assertNotEqual("a", "b")
        self.assertEqual(1.5, b"a")
        self.assertEqual(None, None)
        self.assertEqual()
        self.assertTrue()
        self.assertEquals(1, 1)
        self.assertEqual(-1, 1)
        self.assertEqual((1,), 1)
        self.assertEqual(first=1, second=1)
        self.assertTrue(expr=1)
        self.assertTrue(f"x")
        self.assertTrue(...)
        unittest.TestCase.assertTrue(self, 1)


class NotTC:
    def test(self):
        self.assertTrue(1)


def raiser(x):
    raise ValueError("%s is bad", x)
    raise ValueError("{} is bad", x)
    raise ValueError("{ is bad", x)
    raise ValueError("a" "%s", x)
    raise ValueError(f"%s", x)
    raise ValueError("%s")
    raise ValueError("%s", x=1)
    raise ValueError("%s", *x)
    raise ValueError(x, "%s")
    raise x.err("100%", 1)
    raise (ValueError("%s", 1))
    raise ValueError("%s", 1) from x
    raise ValueError(b"%s", 1)


@dataclass
class D1:
    a: int = field(default=1)
    b: int = dataclasses.field(default=1)
    c = field(default=1)
    d: int = field(default=1) + 1

    def m(self):
        return field()


class D2:
    a: int = field(default=1)
    b = dataclasses.field()


def make():
    x = field()
    dataclasses.make_dataclass("X", [("a", int, field())])
    foo(field())
    return field


@dataclasses.dataclass(frozen=True)
class D3:
    a: int = field(default_factory=lambda: field())
    b: list = field(default_factory=lambda: [field()])
