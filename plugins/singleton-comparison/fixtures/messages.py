"""Singleton comparisons: the message quotes the operands as astroid prints them."""
# pylint: disable=missing-function-docstring,unused-argument,invalid-name,comparison-of-constants,literal-comparison

items = []


def quotes(frame):
    return frame["lat"] != None, frame["a"] == "b" or frame["it's"] == None, frame['single'] == None


def order(r):
    assert False == r.valid
    return None == r.valid, None != r.valid, True == r, False != r


def both(r):
    return None == None, True != None, False == True


def truthiness(x, y):
    if x == True:
        pass
    elif y != False:
        pass
    while x == True:
        break
    z = 1 if x == True else 2
    z = (x == True) if y else 2
    z = 1 if y else (x == True)
    assert x == True
    assert y, x == True
    found = [i for i in items if i == True]
    found = [i == True for i in items]
    found = [i for i in items == True]
    found = {i: 1 for i in items if i != False}
    ok = bool(x == True)
    ok = bool(x != False)
    ok = f(x == True)
    ok = f(a=x == True)
    ok = x == True and y
    ok = not x == True
    ok = [x == True]
    return x == True, x != True, x == False, x != False


def f(*args, **kwargs):
    return args, kwargs


def silent(x):
    if x is None or x is True:
        pass
    return x == 1, x != "a", 1 == x, x < None


def chained(x):
    return None == x == True, x == None == x
