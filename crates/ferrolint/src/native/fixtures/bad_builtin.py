import builtins
from functools import reduce

items = [1, 2, 3]

doubled = map(lambda x: x * 2, items)
evens = filter(None, items)
total = reduce(lambda a, b: a + b, items)
text = input("prompt")
size = len(items)
as_list = list(map(str, items))
both = map(str, filter(None, items))
qualified = builtins.map(str, items)
bound = items.map(str)
called = (map)(str, items)
callback = map


def function():
    return map(str, items)


def shadowing_parameter(filter):
    return filter(items)


def shadowing_local():
    map = dict
    return map(a=1)


def not_shadowed_by_the_outer_function():
    filter = list

    def inner():
        return filter(items)

    return inner


def comprehension_target():
    return [map(x) for map in (str,) for x in items]


def later_global():
    return sorted(items), reduce


class Klass:
    filter = staticmethod(list)
    attribute = map(str, items)
    other = filter(items)

    def method(self):
        return map(str, self), filter(None, self)

    def uses_the_class_name_only_in_the_body(self):
        return (lambda: map(str, items))()


def after():
    return map(str, items)


lazy = lambda: filter(None, items)
lazy_arg = lambda map: map(items)
