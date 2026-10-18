from __future__ import annotations

import os
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections import OrderedDict

count = 0
name = "x"
_private = 1
unused_thing = 2


def fixture(count, name=None, *, _private, unused_thing):
    return count, name, _private, unused_thing


def local_assignment():
    count = 1
    for name in range(3):
        pass
    with open("f") as os:
        pass
    return count, os


def shadows_class_and_function():
    def helper():
        pass

    class Klass:
        pass

    return helper, Klass


def helper():
    pass


class Klass:
    def method(self, helper):
        return helper

    def nested(self):
        helper = 1
        return helper


def uses_global():
    global count
    count = 2


def comprehension_only():
    return [count for count in range(3)]


def type_checking(OrderedDict):
    return OrderedDict


def future(annotations):
    return annotations


def args_ignored(ignored_one, unused_two, dummy, _x):
    ignored_one = 1
    return ignored_one, unused_two, dummy, _x


try:
    pass
except ValueError as error:
    pass


def error_param(error):
    return error


try:
    pass
except ValueError as outer:
    try:
        pass
    except KeyError as outer:
        pass
    try:
        pass
    except KeyError as inner:
        try:
            pass
        except IndexError as inner:
            pass

dummy = 0
unused_other = 0


def dummies(dummy, unused_other):
    dummy = 1
    unused_other = 2
    return dummy, unused_other


def lambda_param():
    return lambda count: count


async def awaited(name):
    return name


def import_local():
    import os
    from sys import path as count
    return os, count


def nested_function():
    def inner(name):
        return name

    return inner


def augmented():
    count = 0
    count += 1
    return count


def global_via_function():
    global later_global
    later_global = 1


def uses_later(later_global):
    return later_global


def nonlocal_assignment():
    count = 1

    def inner():
        nonlocal count
        count = 2

    def reader():
        nonlocal name
        return name

    return inner, reader
