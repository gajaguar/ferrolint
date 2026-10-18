import json
import os.path
from typing import Any as Anything


def uses_locals(used):
    unused = 1
    other = 2
    print(used, other)
    return locals()


def locals_before(used):
    print(locals())
    unused = 1
    return used


def ignored_names():
    _ = 1
    dummy_value = 2
    unused_x = 3
    ignored_y = 4
    _private_name = 5
    return locals()


def imports():
    import json
    import os.path
    import re as regex
    from typing import Any as Anything
    from sys import path
    return locals()


def nested_scopes():
    def inner():
        return 1

    @staticmethod
    def decorated():
        return 2

    class Klass:
        pass

    result = [x for x in range(3)]
    return locals()


def handler():
    try:
        pass
    except ValueError as error:
        print(error)
    except KeyError as other:
        pass
    return locals()


def loop_targets(values):
    for item in values:
        pass
    with open("f") as stream:
        pass
    first, second = values
    total = 0
    total += 1
    return locals()


def global_and_nonlocal():
    global shared
    shared = 1
    captured = 1

    def inner():
        nonlocal captured
        captured = 2

    return locals(), inner


def raises(unused):
    value = 1
    raise ValueError(locals())


def lambda_locals():
    value = 1
    return lambda: locals()


def class_locals():
    value = 1

    class Klass:
        data = locals()

    return Klass
