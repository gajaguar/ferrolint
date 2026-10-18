def f(a, b, c, d):
    if (a):
        pass
    if (a and b):
        pass
    if (a, b):
        pass
    if not (a):
        pass
    if a is not (b):
        pass
    while (a):
        pass
    for x in (a):
        pass
    for x in (a, b):
        pass
    for x in (1):
        pass
    for x in ("a"):
        pass
    for (x) in a:
        pass
    x = (a)
    x = (a, b)
    x = (a + b) * c
    x = (a + b)
    x = ((a))
    x = (
        a
    )
    if ((a)):
        pass
    if (a) and b:
        pass
    if (yield):
        pass
    return (a)


def g(a, b):
    return (a)
    return (a, b)
    return (a if b else a)
    return ((a for a in b))
    return (not a)
    return ()
    del (a)
    del (a), b
    assert (a)
    assert (a), "x"
    raise (a)
    yield (a)
    print((a))
    f(x=(a))
    f(x=(a), y=1)
    f((a))
    y = [i for i in (a)]
    y = [i for i in (a) if (b)]
    y = [i for i in x if (a)]
    y = a if (b) else c
    y = a if b else (c)
    if (n := 10) > 5:
        pass
    if (n := 10):
        pass
    if (a if b else c):
        pass
    if (a) if b else c:
        pass
    with (open("x")) as f:
        pass
    with (open("x") as f, open("y") as g):
        pass
    try:
        pass
    except (ValueError):
        pass
    except (ValueError, TypeError):
        pass


def h(a, b):
    if a:
        pass
    elif (b):
        pass
    x: int = (1)
    x += (1)
    lam = lambda: (1)
    print(f"{(a)}")
    return not (a)
