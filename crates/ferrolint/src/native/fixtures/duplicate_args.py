def plain(a, b):
    return a, b


def twice(a, a):
    return a


def mixed(a, /, b, *a, c, **b):
    return a, b, c


class K:
    async def method(self, x, *, x):
        def inner(y, y, y):
            return y

        return inner
