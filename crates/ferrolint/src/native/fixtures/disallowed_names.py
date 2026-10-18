import os

foo = 1
bar = "text"
baz = None
toto = [1, 2]
tutu = os.getcwd()
tata: int = 3
fine: int
foo2, bar = 1, 2
(qux, baz) = [1, 2]
for foo in range(3):
    pass
with open("f") as bar:
    pass


class foo:
    bar = 1
    baz: int = 2
    toto, tutu = 1, 2

    def __init__(self, foo, bar, *baz, tata=1, **toto):
        self.foo = 1
        self.bar += 1
        self.fine = 3
        self.tutu: int = 4
        (self.tata, other) = (1, 2)

    def tata(self):
        pass

    @classmethod
    def method(cls, foo):
        cls.bar = 1
        return foo

    @staticmethod
    def helper(toto):
        return toto


class Child(foo):
    foo = 1
    bar = 2
    qux = 3

    def __init__(self, tutu):
        self.foo = 2
        self.new_attr = 1
        self.baz = 3

    def tata(self, toto):
        pass

    def other(self, toto):
        pass


def bar(foo, bar=1, *baz, tata, **toto):
    foo = 1
    bar = 2
    baz = 3
    tata = 4
    toto = 5
    tutu = [foo for foo in range(3)]
    x = (bar for bar in range(3))
    for foo in range(3):
        pass
    with open("f") as toto:
        pass
    try:
        pass
    except ValueError as baz:
        pass
    if (tutu := 1):
        pass
    lam = lambda foo: foo
    return foo, bar, baz, tata, toto, tutu, x, lam


async def toto():
    tata = 1
    async with open("f") as baz:
        pass
    return tata, baz


try:
    import simplejson as foo
except ImportError:
    foo = None

try:
    import json as tata
except ImportError:
    tata = None


def fallback():
    try:
        import json as tata
    except ImportError:
        tata = None
    try:
        import json
    except ImportError:
        toto = None
    return tata, json, toto


if os.name == "nt":
    foo = 1
else:
    baz = 2


def nested():
    def tutu():
        pass

    class tata:
        toto = 1

    return tutu, tata


x = lambda baz: baz
y = [toto for toto in range(3)]
z = {tata: 1 for tata in range(3)}
