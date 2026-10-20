"""E0240 inconsistent-mro: bases whose linearizations cannot be merged."""


class A:
    pass


class B(A):
    pass


class C(B):
    pass


class Bad(A, B):  # positive: A comes before its subclass B
    pass


class BadChild(Bad):  # positive: derives from an inconsistent class
    pass


class BadObject(object, A):  # positive: object is an ancestor of A
    pass


class BadDeep(A, C):  # positive: C derives from A through B
    pass


class Chain(B, C):  # positive: B comes before its subclass C
    pass


class Good(B, A):
    pass


class GoodObject(A, object):
    pass


class Left(A):
    pass


class Right(A):
    pass


class Join(Left, Right):
    pass


class Cross1(Left, Right):
    pass


class Cross2(Right, Left):
    pass


class Crossed(Cross1, Cross2):  # positive: the two orders of Left and Right clash
    pass


class Unknown(unknown_base, A, B):  # positive: the unknown base is left out
    pass


class UnknownOnly(unknown_base, A):  # negative
    pass


class Plain(A):
    pass


class Many(Plain, A, B):  # positive: A comes before B, and B derives from A
    pass


class Duplicate(A, A):  # negative: duplicate-bases, not this rule
    pass


class DuplicateChild(Duplicate):  # negative
    pass


class SelfBase(A, object):  # negative
    pass


def local():
    class Inner(A):
        pass

    class Worse(A, Inner):  # positive
        pass

    return Worse
