class Rebound:
    def __init__(self):
        self.a = 1


def rebound():
    x = Rebound()
    x.b = 1
    x = Rebound()
    x.c = 1


class Parser:
    class _Result:
        def __init__(self):
            self.a = 1

    def parse(self):
        res = self._Result()
        res.b = 1
        return res

    @classmethod
    def build(cls):
        res = cls._Result()
        res.c = 1
        return res


class Outer:
    def __init__(self):
        self.a = 1

    def run(self):
        test = self

        class Inner:
            def go(self):
                test.b = 1
                self.c = 1

        return Inner


class Special:
    def convert(self):
        self.__class__ = Special
        self.__doc__ = "doc"


class Callee:
    def __new__(cls):
        inst = object.__new__(cls)
        inst.setup()
        return inst

    def setup(self):
        self.a = 1


class Delegated:
    def __init__(self, other):
        self.helper.setup()
        other.prepare()
        self.fill()

    def fill(self):
        self.b = 1

    def setup(self):
        self.a = 1

    def prepare(self):
        self.c = 1


class Aliased:
    def __init__(self):
        self.start()

    def _begin(self):
        self.a = 1

    def _other(self):
        self.b = 1

    start = _begin


def make_mixin():
    class Made:
        def __init__(self):
            self.ready = 1

        def load(self):
            self.loaded = 1

    return Made


Mixin = make_mixin()


class FromFactory(Mixin):
    def run(self):
        self.ready = 2
        self.loaded = 2
        self.own = 1
