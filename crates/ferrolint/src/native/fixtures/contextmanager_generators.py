import contextlib
from contextlib import contextmanager


@contextmanager
def cm():
    x = object()
    yield x
    print("cleanup")


@contextmanager
def cm_const():
    yield 1
    print("cleanup")


@contextmanager
def cm_finally():
    x = object()
    try:
        yield x
    finally:
        print("cleanup")


@contextmanager
def cm_try_except():
    x = object()
    try:
        yield x
    except Exception:
        print("x")


@contextmanager
def cm_last():
    x = object()
    yield x


@contextmanager
def cm_no_try():
    x = object()
    if x:
        yield x
    print("after")


@contextlib.contextmanager
def cm2():
    x = object()
    yield x
    print("cleanup")


def plain():
    x = object()
    yield x
    print("cleanup")


class K:
    @contextmanager
    def m(self):
        x = object()
        yield x
        print("cleanup")

    def use(self):
        with self.m() as v:
            yield v


def gen1():
    with cm() as c:
        yield c


def gen2():
    with cm_const() as c:
        yield c


def gen3():
    with cm_finally() as c:
        yield c


def gen4():
    with cm_try_except() as c:
        yield c


def gen5():
    with cm_last() as c:
        yield c


def gen6():
    with cm_no_try() as c:
        yield c


def gen7():
    with cm2() as c:
        yield c


def gen8():
    with plain() as c:
        yield c


def gen9():
    with cm():
        yield 1


def gen10():
    with cm() as c:
        pass
    yield 1


def gen11():
    with cm() as c:
        yield


def gen12():
    with cm() as c, cm2() as d:
        yield c


async def gen13():
    with cm() as c:
        yield c


def gen14():
    with cm() as c:
        yield 1
