import abc


class A:
    def __repr__(self):
        return 1

    def __format__(self, spec):
        return None

    def __iter__(self):
        return 1

    def __length_hint__(self):
        return -1

    def __getnewargs__(self):
        return [1]


class B:
    def __repr__(self):
        pass

    def __format__(self, spec):
        if spec:
            return 1
        return 2

    def __iter__(self):
        yield 1

    def __length_hint__(self):
        return 1.5

    def __getnewargs__(self):
        return ()


class C:
    def __repr__(self):
        raise NotImplementedError

    def __iter__(self):
        return []

    def __length_hint__(self):
        return -(1)

    def __length_hint__(self):
        return True

    def __length_hint__(self):
        return NotImplemented

    def __repr__(self):
        print(1)

    def __repr__(self):
        """doc."""
        print(1)

    def __repr__(self):
        ...

    def __repr__(self):
        """doc."""

    @abc.abstractmethod
    def __repr__(self):
        print(1)

    def __repr__(self):
        return

    def __repr__(self):
        yield 1

    def __repr__(self):
        return (yield)

    def __repr__(self):
        return [(yield)]

    def __iter__(self):
        return [(yield)]

    def __repr__(self):
        return lambda: 1

    def __repr__(self):
        return {1}

    def __repr__(self):
        return {}

    def __repr__(self):
        return 1,

    def __repr__(self):
        return ...

    def __repr__(self):
        return b"a"

    def __format__(self, s):
        return f"{s}"

    def __format__(self, s):
        return f"a"

    def __format__(self, s):
        return str(s)

    def __length_hint__(self):
        return f"{x}"

    def __length_hint__(self):
        return f"a"

    def __repr__(self):
        return "a" "b"

    def __repr__(self):
        return self.x

    @staticmethod
    def __repr__():
        return 1

    @classmethod
    def __repr__(cls):
        return 1

    async def __format__(self, s):
        return 1


class D:
    def __iter__(self):
        return self

    def __repr__(self):
        return self

    def __length_hint__(self):
        return self


class E:
    def __iter__(self):
        return self

    def __next__(self):
        return 1


class F(E):
    def __iter__(self):
        return self


class H:
    def __getnewargs_ex__(self):
        return ((), {})

    def __getnewargs_ex__(self):
        return ((), {"a": 1})

    def __getnewargs_ex__(self):
        return ([], {})

    def __getnewargs_ex__(self):
        return ((), [])

    def __getnewargs_ex__(self):
        return ((), {} if x else 1)

    def __getnewargs_ex__(self):
        return (self.args, self.kwargs)

    def __getnewargs_ex__(self):
        return (a, b)

    def __getnewargs_ex__(self):
        return (), x

    def __getnewargs_ex__(self):
        return ((*a,), {})

    def __getnewargs_ex__(self):
        return ((), {k: 1 for k in x})

    def __getnewargs_ex__(self):
        return (1, 2)

    def __getnewargs_ex__(self):
        return ()

    def __getnewargs_ex__(self):
        return [1]

    def __getnewargs_ex__(self):
        return ((), {}, 1)
