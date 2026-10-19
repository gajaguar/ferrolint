"""C1804 and C1805: a comparison with an empty string or with zero."""


def tests(x, y, a, b):
    if x == "":
        pass
    if "" == x:
        pass
    if x != "":
        pass
    if x is "":  # noqa
        pass
    if x is not "":  # noqa
        pass
    if x == 0:
        pass
    if 0 == x:
        pass
    if x != 0:
        pass
    if x is 0:  # noqa
        pass
    if x is not 0:  # noqa
        pass
    if x == 0.0:
        pass
    if x == 0j:
        pass
    if x == 00:
        pass
    if x.attr[1] == 0:
        pass
    if x(1) != "":
        pass
    if x == False:  # noqa
        pass
    if x == b"":
        pass
    if x == f"":
        pass
    if x == " ":
        pass
    if x == 1:
        pass
    if x < 0:
        pass
    if x in "":
        pass
    if x == "" == y:
        pass
    if x == "" and y != 0:
        pass
    if "" == "":
        pass
    if 0 == 0:
        pass
    if "" == 0:
        pass
    if 0 != "":
        pass
    return x


def contexts(x, y):
    z = x == ""
    z = x != 0
    z = x != "" and y != 0
    z = not x != 0
    z = bool(x != "")
    z = a if x != "" else y
    z = [i for i in y if i != 0]
    z = [i != 0 for i in y]
    z = all(i != "" for i in y)
    z = list(filter(lambda i: i != 0, y))
    z = (x != 0) + 1
    while x != "":
        break
    assert x != 0
    assert x, y != 0
    return z
