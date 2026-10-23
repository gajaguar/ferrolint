import abc


def pair():
    return 1, 2, 3


def pair_branch(x):
    if x:
        return (1, 2, 3)
    return (4, 5)


def pair_cond(x):
    if x:
        return (1, 2, 3)


def gen():
    yield 1, 2, 3


def lst():
    return [1, 2, 3]


def dct():
    return {"a": 1, "b": 2, "c": 3}


def nested():
    def inner():
        return 1, 2, 3
    return inner()


a, b = (1, 2, 3)
a, b = 1, 2, 3
t = (1, 2, 3)
a, b = t
[a, b] = t
a, b = [1, 2, 3]
a, b = {1, 2, 3}
a, b = pair()
a, b = pair_branch(1)
a, b = pair_cond(1)
a, b = gen()
a, b = lst()
a, b = dct()
a, b = nested()
a, *b = t
a, b = {"x": 1, "y": 2, "z": 3}
d = {"x": 1, "y": 2, "z": 3}
a, b = d
a, b = d.items()
a, b = d.keys()
a, b = d.values()
a, (b, c) = (1, (2, 3), 4)
a = b = (1, 2, 3)
(a, b) = t
a, b = "abc"
a, b = (1, 2)
x = (1,
     2, 3)
a, b = x
a, b, c = t


class C:
    def m(self):
        return 1, 2, 3

    def use(self):
        a, b = self.m()
        a, b = t
        return a, b

    @staticmethod
    def st():
        return 1, 2, 3

    def use2(self):
        a, b = self.st()


class Abs(abc.ABC):
    @abc.abstractmethod
    def f(self): ...

    def g(self):
        a, b = (1, 2, 3)


def func_local():
    t3 = (1, 2, 3)
    a, b = t3
    for i in range(2):
        t4 = (1,)
    a, b = t4
    return a, b


def default(p=(1, 2, 3)):
    a, b = p


def star(*args):
    a, b = args


a, b = pair_branch
for a, b in d.items():
    pass
for a, b, c in d.items():
    pass
for a, b in d.keys():
    pass
for a, b in d.values():
    pass
for a, b in d:
    pass
for a, b, c in d:
    pass
for a, b in {1: 2}.keys():
    pass
for a, b in {"ab": (1, 2), "cd": (1, 2)}.keys():
    pass
for a, b in {"x": (1, 2, 3)}.values():
    pass
for a, *b in {"x": (1, 2, 3)}.values():
    pass
for a, b in {"x": "abc"}.values():
    pass
for a, b, c in {1: 2}.items():
    pass
for a, b, c in {1: 2}:
    pass
for a, b, c in {"a": 1}.items():
    pass


def two(x):
    if x:
        return (1, 2, 3)
    return (4, 5, 6)


def two_same_len_diff(x):
    if x:
        return (1, 2, 3)
    return (4, 5)


def names(x, y):
    return (x, y, 1)


def with_none(x):
    if x:
        return (1, 2, 3)
    return None


def with_bare(x):
    if x:
        return (1, 2, 3)
    return


def with_raise(x):
    if x:
        return (1, 2, 3)
    raise ValueError


def with_try(x):
    try:
        return (1, 2, 3)
    finally:
        pass


def deco_ok():
    return 1, 2, 3


class K:
    def __init__(self):
        self.v = (1, 2, 3)

    def m(self):
        return (1, 2, 3)

    def uses(self):
        a, b = self.m()
        a, b = self.m


def call_name(x, y):
    a, b = (x, y, 1)
    a, b = two(1)
    a, b = two_same_len_diff(1)
    a, b = names(1, 2)
    a, b = with_none(1)
    a, b = with_bare(1)
    a, b = with_raise(1)
    a, b = with_try(1)
    a, b = deco_ok()
    a, b = ((1, 2), 3, 4)
    a, b = (x.y, 3, 4)
    a, b = (x, 3, 4)
    a, b = [x for x in (1, 2, 3)]
    a, b = (1, "s", 3.5, True, None, -1)
    a, b = (1,
            2,
            3)
    a, b = (lambda: 1, 2, 3)
    a, b = ({1: 2}, "a b", x)
    a, b = (x + 1, [x], {x}, {x: 1}, not x)
    a = b = c = 5
    return a, b


def shadows():
    d = {"a": 1, "b": 2, "c": 3}
    d.update(z=1)
    a, b = d
    e = {"a": 1, "b": 2, "c": 3}
    a, b = e
    for k, v, w in d.items():
        pass
    for k, v in d.items():
        pass
    for (k, v) in e.items():
        pass
    for k, v, w in e.items():
        pass
    for k, v in e.keys():
        pass
    for k, v in e.values():
        pass
    for k, v in zip(d, d):
        pass
    for *k, v in {"a": 1}.values():
        pass
    for k, *v in {"a": 1}.values():
        pass
    for k, *v, w in {"a": 1}.values():
        pass
    for k, v in {"a": (1, 2), "b": (1, 2, 3)}.values():
        pass
    for k, v in {"a": x for x in "abc"}.values():
        pass
    for k, v in {"a": [1, 2], "b": b"abc"}.values():
        pass
    for k, v in {"a": d[0:2]}.values():
        pass
    for k, v in {"a": d[0:3]}.values():
        pass
    for k, v in {"a": d[0:6:2]}.values():
        pass
    for k, v in {"a": d[0:4:2]}.values():
        pass
    for k, v in {"a": d[0:5:0]}.values():
        pass
    for k, v in {(1, 2): 1}.keys():
        pass
    for k, v in {"ab": 1, "abc": 2}.keys():
        pass


def varargs(first, *args):
    a, b = args
    c, d = args[:2]
    e, f = args[1:]
    g, h = first[:2]
    return a, b, c, d, e, f, g, h


def shadowed(*args):
    args = (1, 2, 3)
    i, j = args[:2]
    return i, j
