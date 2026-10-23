import os


def func():
    return 1


def nothing():
    pass


def gen():
    yield 1


class Klass:
    pass


lam = lambda: 1
genexp = (x for x in range(3))
number = 1
a = b = c = x = y = z = 3


def tests():
    if True:
        pass
    if 1:
        pass
    if "s":
        pass
    if None:
        pass
    if []:
        pass
    if {}:
        pass
    if ():
        pass
    if {1}:
        pass
    if lambda: 1:
        pass
    if (i for i in range(3)):
        pass
    if func:
        pass
    if nothing:
        pass
    if gen:
        pass
    if Klass:
        pass
    if lam:
        pass
    if genexp:
        pass
    if number:
        pass
    if os:
        pass
    if func():
        pass
    if not func:
        pass
    if func and a:
        pass
    elif 5:
        pass
    elif func:
        pass
    v = 1 if func else 2
    w = 1 if "a" else 2
    l = [i for i in range(3) if func]
    m = [i for i in range(3) if 0]
    n = {i: 1 for i in range(3) if i and 1}
    if f"x":
        pass
    if b"x":
        pass
    if ...:
        pass
    if print:
        pass
    if range:
        pass


def gen_returning():
    return (i for i in range(3))


def gen_call():
    if gen_returning():
        pass
    return [1 for _ in range(3) if gen_returning()]


def simplify():
    r1 = a and 0 or b
    r2 = a and None or b
    r3 = a and "" or b
    r4 = a and [] or b
    r5 = a and False or b
    r6 = a and 1 or b
    r7 = a and x or b
    r8 = a and f() or b
    r9 = a and 0 or b and c
    r10 = a and (1 < 2) or b
    return a and 0 or b


def condition_tests():
    if a and True:
        pass
    if a or True:
        pass
    if a and False:
        pass
    if a or False:
        pass
    if a and 1 and b:
        pass
    if (a or 1) and b:
        pass
    if a and (b or 0):
        pass
    if True and False:
        pass
    if a and b:
        pass
    while a or 1:
        pass
    assert a and 1
    w = bool(a and 1)
    k = [i for i in range(3) if i and 0]
    q = 1 if a or 1 else 2
    z = a and 1
    if not (a and 1):
        pass
    if a or 0 or b or ():
        pass
    if "x" and a:
        pass
    if a and (not 0):
        pass
    if a and -1:
        pass
    if a and -0:
        pass


def cycles():
    if a >= b and b >= a:
        pass
    if a >= b and b >= c and c >= a:
        pass
    if a > b and b > a:
        pass
    if a > 5 and a < 3:
        pass
    if a > 3 and a < 5:
        pass
    if a == 1 and a == 2:
        pass
    if a <= b and b <= c and c <= a:
        pass
    if a >= b and b >= c and c == a:
        pass
    if a < b < a:
        pass
    if a >= b >= c >= a:
        pass
    if a <= 5 and a >= 5:
        pass
    if x.y >= 5 and x.y <= 5:
        pass
    if f() >= 5 and f() <= 3:
        pass
    if a != b and b != a:
        pass
    if a >= a and a <= a:
        pass
    if a >= b and a <= b:
        pass
    if a > b and b > c and c > d:
        pass


def chain():
    if 0 < a is None:
        pass
    if a < b in c:
        pass
    if a is b is c:
        pass
    if a < b < c:
        pass
    if a in b not in c:
        pass
    if a == b != c is d:
        pass
    if a is not b == c:
        pass


def neg():
    if not not a:
        pass
    if not (a == b):
        pass
    if not (1 == 2):
        pass
    if not (1 < 2):
        pass
    if not ("a" in "abc"):
        pass
    if not ([1] is [2]):
        pass
    if not (None is None):
        pass
    if not (a is None):
        pass
    if not (1 in {1}):
        pass
    if not (1 == 1 == 1):
        pass
    if not (a not in b):
        pass
    if not 1 == 1:
        pass
    if not not (1 == 2):
        pass


class Eq:
    def __ne__(self, other):
        return not (1 == 2)

    def __eq__(self, other):
        return not (1 == 2)


def multiline(a):
    if a.get(
        'k',   1) or True:
        pass


class Attributes:
    table = {}
    names = []

    def __init__(self):
        self.cells = {}
        self.names.append(1)

    @classmethod
    def build(cls, key, other):
        if not 'k' in cls.table:
            pass
        if not 'k' in cls.__dict__:
            pass
        if not 'k' in other.table:
            pass

    def find(self, key):
        if not 'k' in self.cells:
            pass


def named_call(value):
    text = str(value)
    if not 'k' in text:
        pass


class Rebound:
    grown = 1
    pair = 1
    prop = 1
    plain = 1

    def __init__(self):
        self.grown += 1.5
        self.pair, other = None, 1

    @property
    def prop(self):
        return object()

    @staticmethod
    def build(self):
        if not self.plain < 3:
            pass

    def find(self):
        if not self.grown < 3:
            pass
        if not self.pair < 3:
            pass
        if not self.prop < 3:
            pass
        if not self.plain < 3:
            pass
