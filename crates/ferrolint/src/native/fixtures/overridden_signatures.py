class A:
    def m1(self, a):
        return a

    def m2(self, a, *, k):
        return a, k

    def m3(self, a, b):
        return a, b

    def m4(self, a, *args, **kwargs):
        return a, args, kwargs

    def m5(self, a):
        return a

    def m6(self, a, b):
        return a, b

    def __eq__(self, other):
        return other

    def __getitem__(self, key):
        return key

    def __new__(cls, a):
        return a

    def m7(self, a, b):
        return a, b

    def m8(self, *, k):
        return k

    def m9(self, a, /, b):
        return a, b

    def __m10(self, a):
        return a

    def m11(self, a):
        return a

    def m12(self, a, b):
        return a, b


class B(A):
    def m1(this, a):
        return a

    def m2(self, a, *, j):
        return a, j

    def m3(self, *args):
        return args

    def m4(self, a):
        return a

    def m5(self, a, b=1, *, c=2):
        return a, b, c

    def m6(self, _a, c):
        return _a, c

    def __eq__(self, x, y):
        return x, y

    def __getitem__(self, k):
        return k

    def __new__(cls):
        return 1

    def m7(self, b, a):
        return a, b

    def m8(self, **kwargs):
        return kwargs

    def m9(self, x, /, b):
        return x, b

    def __m10(self):
        return 1

    async def m11(self, a, b):
        return a, b

    def m12(self, x, y, z):
        return x, y, z


class I:
    def m(self, a, b=1):
        return a, b

    @classmethod
    def c(cls, a):
        return a

    @staticmethod
    def s(a):
        return a

    @property
    def p(self):
        return 1

    @p.setter
    def p(self, value):
        del value

    def over(self, a):
        return a

    attr = 1


class J(I):
    def m(this, a):
        return a

    @classmethod
    def c(klass, b):
        return b

    @staticmethod
    def s(b):
        return b

    @p.setter
    def p(self, new):
        del new

    def attr(self, x):
        return x

    def over(self, a, b):
        return a, b

    def unknown(self, a):
        return a


class K(J):
    def m(self, x, y, z):
        return x, y, z

    def unknown(self):
        return 1

    def dummy(self, _ignored):
        return 1


class L(K):
    def dummy(self, value):
        return value


class M(dict):
    def get(self, a, b, c):
        return a, b, c
