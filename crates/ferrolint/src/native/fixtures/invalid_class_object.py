from somewhere import Unknown


def func():
    pass


async def afunc():
    pass


class Real:
    pass


class Meta(type):
    pass


class WithMeta(metaclass=Meta):
    pass


class New:
    def __new__(cls):
        return 1


number = 1
twice = 1
twice = 2
items = [1]
text = "x"


class Holder:
    def fine(self):
        self.__class__ = Real
        self.__class__ = Holder
        self.__class__ = Unknown
        self.__class__ = Unknown()

    def bad(self, other):
        self.__class__ = 1
        self.__class__ = "x"
        self.__class__ = b"x"
        self.__class__ = None
        self.__class__ = True
        self.__class__ = 1.5
        self.__class__ = ...
        self.__class__ = [1]
        self.__class__ = (1, 2)
        self.__class__ = {1: 2}
        self.__class__ = {1}
        self.__class__ = func
        self.__class__ = afunc
        self.__class__ = lambda: 1
        self.__class__ = Real()
        self.__class__ = New()
        self.__class__ = f"a{other}"
        self.__class__ = number
        self.__class__ = items
        self.__class__ = text
        self.__class__ = len
        self.__class__ = self.bad
        self.__class__ = -1
        self.__class__ = 1 + 2
        self.__class__ = [x for x in other]
        self.__class__ = other
        self.__class__ = int
        self.__class__ = type(other)
        self.__class__ = print
        self.__class__ = Holder.bad
        self.__class__ = not other
        self.__class__ = other or 1

    def multi(self, other):
        self.__class__ = other.__class__ = 5
        other.__class__ = 7
        self.__class__, x = 1, Real
        x, self.__class__ = 1, Real
        x, self.__class__ = Real, 1
        self.__class__, other.__class__ = Real, 1
        self.__class__, other.__class__ = 1, Real
        (a, self.__class__), b = (1, 2), 3
        self.__class__ += 1
        self.__class__: int = 5
        self.__class__: int
        for self.__class__ in [1]:
            pass
        with open("f") as self.__class__:
            pass
        self.__class__, *x = 1, 2
        self.__class__, y = other
        self.__class__ = y = 1
        [self.__class__, x] = [1, Real]
        self.__class__, x = (1, 2), Real

    def other_names(self):
        number.__class__ = 3
        Real().__class__ = 3
        Real.__class__ = 3
        Holder.__class__ = 3
