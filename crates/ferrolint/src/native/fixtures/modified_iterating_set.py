def display():
    s = {1, 2, 3}
    for x in s:
        s.add(x + 10)
    for x in s:
        s.remove(x)
    for x in s:
        s.discard(x)
    for x in s:
        s.pop()
    for x in s:
        s.clear()
    for x in s:
        del s


def builtin_calls():
    empty = set()
    for x in empty:
        empty.add(x)
    full = set([1, 2])
    for x in full:
        full.discard(x)
    names = set(["a", "b"])
    for x in names:
        if x:
            names.add("c")


def deleting_the_target():
    s = {1, 2}
    for x in s:
        del x
    for x in s:
        del s


def safe(other):
    s = {1, 2, 3}
    for x in s.copy():
        s.add(x)
    for x in s:
        other.add(x)
    for x in s:
        s.update([x])
    for x in s:
        s.union([x])
    for x in other:
        other.add(x)
    for x in sorted(s):
        s.add(x)
    lst = [1, 2]
    for x in lst:
        lst.add(x)


def rebound():
    s = {1, 2}
    s = [1, 2]
    for x in s:
        s.append(x)
    for x in s:
        s.add(x)


def dict_and_list_still_work():
    d = {"a": 1}
    for k in d:
        d["b"] = 2
    lst = [1]
    for x in lst:
        lst.append(x)
