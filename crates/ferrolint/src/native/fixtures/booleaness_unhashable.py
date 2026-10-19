"""E1143: a list, dict or set as a dict key or a set member."""

lst = [1]
st = {1}
dct = {1: 2}
tup = (1, [2])
alias = lst
twice = [1]
twice = 1

a1 = {[1]: 1}
a2 = {{1: 2}: 1}
a3 = {{1}: 1}
a4 = {lst: 1}
a5 = {st: 1}
a6 = {dct: 1}
a7 = {alias: 1}
a8 = {tup: 1}
a9 = {twice: 1}
b1 = {(1, [2]): 1}
b2 = {[1]: 1, [2]: 2, 3: 4}
b3 = {[1]: 1,
      [2]: 2}
b4 = {list(): 1, dict(): 2, set(): 3, tuple(): 4, str(): 5}
b5 = {[i for i in lst]: 1}
b6 = {{i for i in lst}: 1}
b7 = {{i: i for i in lst}: 1}
b8 = {**dct, [1]: 2}
b9 = {1.5: 1, "a": 2, None: 3, 1j: 4, ...: 5, f"{lst}": 6}
c1 = {[1], 2}
c2 = {lst}
c3 = {frozenset(lst), tuple(lst), (1, 2), "a"}
c4 = {*lst}
c5 = {[1] for _ in lst}
c6 = {[1]: 1 for _ in lst}
c7 = {[[1]]: 1}
c8 = {{}: 1, (): 2}
d1 = {}[[1]]
d2 = {1: 2}[[1]]
d3 = {1: 2}[{1}]
d4 = dct[[1]]
d5 = {lst: 1}[lst]
d6 = {1: 2}[1]
d7 = {1: 2}[(1, [2])]
{[1]: 1}[[2]] = 3
del {1: 2}[[1]]


def local():
    items = []
    return {items: 1}, {items}


class Holder:
    key = {}
    table = {key: 1}

    def method(self):
        return {lst: 1}
