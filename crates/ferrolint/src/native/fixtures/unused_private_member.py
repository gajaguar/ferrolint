class Plain:
    __slots_like = 1
    _kept = 2
    __dead = 3
    __alive = 4

    def __init__(self):
        self.__hidden = 1
        self.__shown = 2
        self.__only_cls = 3
        self.__other = 4
        self._single = 5

    def public(self):
        return self.__shown + self.__alive

    def __unused(self, a, b: int = 1, *rest, key=None, **kw):
        pass

    def __used(self):
        pass

    def call(self):
        return self.__used()

    @staticmethod
    def __static():
        pass

    @classmethod
    def __klass(cls):
        pass

    @classmethod
    def use_klass(cls):
        return cls.__klass()

    def __recursive(self):
        return self.__recursive()

    def __dunder__(self):
        pass

    def _protected(self):
        pass

    async def __coro(self, /, x, *, y: str = "s"):
        pass


class Outer:
    def method(self):
        def __inner():
            pass

        def __called():
            pass

        __called()

        class Nested:
            def __deep(self):
                pass

            __nested_var = 1

        return Nested

    def other(self, obj):
        return obj.__chain


class Attr:
    def __init__(self):
        self.__a = 1
        self.__b = 2

    def read(self, other):
        return other.__b


class ByName:
    __flag = False
    __named = 1

    def use(self):
        return ByName.__flag


class Handler:
    def run(self):
        try:
            pass
        except ValueError as __err:
            pass

        for __item in []:
            pass


class Metaclass(type):
    def __new__(mcs, name, bases, ns):
        obj = super().__new__(mcs, name, bases, ns)
        obj.__registry = {}
        return obj

    def lookup(cls):
        return cls.__registry


class NewStore:
    def __new__(cls):
        inst = object.__new__(cls)
        inst.__made = 1
        inst.__lost = 2
        return inst

    def get(self):
        return self.__made


class Decorated:
    @property
    def __prop(self):
        return 1

    def get(self):
        return self.__prop


class UsesType:
    def __hook(self):
        pass

    def go(self):
        type(self).__hook(self)
