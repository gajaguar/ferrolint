class P:
    __match_args__ = ("x", "y")


class Q:
    __match_args__ = ["x"]


class R:
    __match_args__ = ("x", 1)


class S(P):
    pass


class T:
    __match_args__ = "x"

    def f(self):
        __match_args__ = 5


def m(v):
    match v:
        case x:
            pass
        case 1:
            pass
    match v:
        case 1:
            pass
        case y if y:
            pass
        case z:
            pass
    match v:
        case 1:
            pass
        case _:
            pass
    match v:
        case int(a):
            pass
        case str(b):
            pass
        case bool(c):
            pass
        case list(d) | tuple(d):
            pass
        case int(e, f):
            pass
        case int(x=h):
            pass
        case P(1, 2, 3):
            pass
        case P(1, 2):
            pass
        case P(1):
            pass
        case P(1, x=2):
            pass
        case P(x=1, x=2):
            pass
        case S(1, 2, 3):
            pass
        case Q(1):
            pass
        case R(1):
            pass
        case int(1):
            pass
        case Unknown(1):
            pass
        case P(1, y=2, y=3):
            pass
        case P(y=1, y=2, y=3):
            pass
        case float(1.5) | dict(k):
            pass
