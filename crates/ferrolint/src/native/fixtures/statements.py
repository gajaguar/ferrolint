"""Module docstring."""
"second string"
X = 1
"attribute doc"
Y: int = 2
"attribute doc 2"


class A:
    """Class doc."""
    "not doc"
    z = 1
    "doc of z"

    def __init__(self):
        """Init doc."""
        self.a = 1
        "doc of a"
        "again"

    def other(self):
        """Doc."""
        x = 1
        "no effect"
        if x:
            "in if"
            x = 2
            "after assign"


def f():
    "doc"
    "no"
    b"bytes"
    f"text"
    ...


def w():
    with open("a") as a, open("b"):
        pass
    with open("a") as a, b:
        pass
    with a as b, c as d, e:
        pass
    with a, b:
        pass
    with (a as b, c):
        pass
    with a() as b, c.d:
        pass


async def aw():
    async with a as b, c:
        pass


def en(seq, self, other):
    for i in range(len(seq)):
        print(seq[i])
    for i in range(0, len(seq)):
        print(seq[i])
    for i in range(1, len(seq)):
        print(seq[i])
    for i in range(len(seq)):
        print(other[i])
    for i in range(len(self.items)):
        print(other.items[i])
    for i in range(len(seq), 3):
        print(seq[i])
    for i in range(len(seq)):
        print([seq[i] for _ in seq])
    for i in range(len(seq)):
        def g():
            return seq[i]
    for a, b in range(len(seq)):
        print(seq[a])
    for i in range(len(seq)):
        pass
    for i in range(len(seq[0])):
        print(seq[i])
    for i in range(2, len(seq), 1):
        print(seq[i])
    for i in range(len(seq)):
        seq[j]
    else:
        seq[i]
    for self.i in range(len(seq)):
        seq[i]


class It:
    def __iter__(self):
        for i in range(len(self)):
            yield self[i]


def join(xs):
    r = ""
    for x in xs:
        r += x
    r2 = ""
    for x in xs:
        r2 += f"{x}"
    r3 = ""
    for x in xs:
        r3 += f"{x},"
    r4 = ""
    for x in xs:
        r4 += x + "a"
    r5 = ""
    for x in xs:
        r5 += x
        print(x)
    r6 = 0
    for x in xs:
        r6 += x
    r7 = ""
    for y in xs:
        r7 += x
    r8 = ""
    r8 = ""
    for x in xs:
        r8 -= x
    r9, r10 = "", ""
    for x in xs:
        r10 += x
    r11 = ""
    print()
    for x in xs:
        r11 += x


def modify():
    lst = [1, 2, 3]
    for x in lst:
        lst.append(x)
        lst.remove(x)
        lst.pop()
    d = {"a": 1}
    for k in d:
        d["b"] = 1
        d[k] = 2
    s = {1}
    for e in s:
        s.add(1)
    other = list(lst)
    for x in other:
        other.append(1)
    for x in lst:
        del x
    for k in d:
        del k
    for k, v in d.items():
        d[k] = 3
    for x in lst:
        def inner():
            lst.append(1)
    for x in lst[:]:
        lst.append(x)
    lst2 = []
    lst2 = [1]
    for x in lst2:
        lst2.append(1)


def looping(xs):
    it = iter(xs)
    for a in xs:
        for b in it:
            print(b)
    gen = (x for x in xs)
    for a in xs:
        for b in gen:
            print(b)
    for a in xs:
        print(list(gen))
    for a in xs:
        it2 = iter(xs)
        for b in it2:
            print(b)
    it3 = map(str, xs)
    for a in xs:
        for b in xs:
            print(list(it3))
    it4 = zip(xs, xs)
    for a in xs:
        next(it4)
    it5 = iter(xs)
    for a in it5:
        for b in it5:
            pass
    it6 = iter(xs)
    for a in xs:
        for b in it6:
            break
    it7 = iter(xs)
    for a in xs:
        for b in it7:
            pass
        return
    it8 = iter(xs)
    for a in xs:
        for b in it8:
            pass
        break
    for c in xs:
        for a in xs:
            for b in it8:
                pass
        break


def numbers():
    yield 1


def use_numbers(xs):
    g = numbers()
    for a in xs:
        for b in g:
            pass


def positional(a, b, /, c, d=1):
    pass


def positional2(a, /, **kw):
    pass


def calls():
    positional(1, 2, 3, a=1)
    positional(1, 2, 3, a=1, b=2)
    positional(1, 2, c=3)
    positional2(1, a=1)
    positional(1, 2, 3, **{})

