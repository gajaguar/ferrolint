"""E1144: a slice step of 0 on a sequence."""

lst = [1, 2]
tup = (1, 2)
txt = "abc"
byt = b"abc"
dct = {1: 2}
alias = lst

a1 = lst[::0]
a2 = tup[::0]
a3 = txt[::0]
a4 = byt[::0]
a5 = dct[::0]
a6 = alias[1:2:0]
a7 = [1, 2][::0]
a8 = "abc"[1:2:0]
a9 = lst[::False]
b1 = lst[::0.0]
b2 = lst[::-0]
b3 = lst[::1]
b4 = lst[::0j]
b5 = [i for i in lst][::0]
b6 = (1, 2)[::0x0]
b7 = list(lst)[::0]
b8 = {1, 2}[::0]
b9 = lst[::True]
c1 = lst[::None]
c2 = lst[0]
c3 = lst[::0][::0]
c4 = (lst)[::0]
c5 = lst[0][::0]
c6 = txt[::0] + txt[::0]
c8 = tuple(lst)[::0]
c9 = str(lst)[::0]
d1 = lst[0:1:0, 1]
lst[::0] = [1]
del lst[::0]
tup[::0] = 1
del tup[::0]
txt[::0] = "a"


def unknown(p, q=[1]):
    return p[::0], q[::0]


class K:
    def __getitem__(self, i):
        return i


k = K()
e1 = k[::0]


def local():
    items = [1]
    return items[::0], items[::1]
