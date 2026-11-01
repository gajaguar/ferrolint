from collections import defaultdict


def comp(xs):
    return [x for x in xs]


def tern(a):
    return {} if a else {"k": 1}


def mixed(a):
    if a:
        return []
    return ()


def fall(a):
    if a:
        return []


def bare(a):
    if a:
        return
    return []


def dd(a):
    out = defaultdict(list)
    return out


def rebound(a):
    out = []
    out = {}
    return out


def param(out):
    return out


def unpack():
    return [1], {}


def aug(a):
    out = []
    out += a
    return out


a, b = unpack()
assert comp([]) == []
assert tern(1) == {}
assert mixed(1) == []
assert fall(1) == []
assert bare(1) == []
assert dd(1) == {}
assert rebound(1) == {}
assert param([]) == []
assert b == {}
assert aug([1]) == []
assert [x for x in range(2)] == []
y = [x for x in range(2)]
assert y == []
