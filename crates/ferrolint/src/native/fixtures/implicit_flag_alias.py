import enum
from enum import IntFlag


class P(IntFlag):
    A = 1
    B = 2
    C = 3
    D = 4
    E = 7
    F = 8
    G = H = 12


class Q(enum.IntFlag):
    A = 1
    B = 1
    C = 3
    E = 0
    F = 0
    X = True

    def m(self):
        z = 5


class R(P):
    A = 16
    B = 17


class S(enum.Flag):
    A = 1
    B = 1


class T(IntFlag):
    A = 1
    B = 3
    C = 7
    D = 6
