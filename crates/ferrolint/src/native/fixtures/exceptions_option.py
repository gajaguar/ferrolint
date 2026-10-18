import builtins
from builtins import OSError as IoProblem


class Custom(Exception):
    pass


class Sub(Custom):
    pass


def f():
    try:
        pass
    except Exception:
        pass
    try:
        pass
    except ValueError:
        pass
    try:
        pass
    except (Custom, Sub):
        pass
    try:
        pass
    except builtins.OSError:
        pass
    try:
        pass
    except IoProblem as err:
        print(err)
    try:
        pass
    except IOError:
        pass
    try:
        pass
    except Custom:
        raise
    raise Exception("not general here")


def g():
    raise ValueError("a")


def h():
    raise Custom


def i():
    raise Sub()
