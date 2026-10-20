"""W4903, W4905 and W4906 against Pylint 4.1.3 on Python 3.14."""
# pylint: disable=unused-import,import-error,missing-function-docstring,wrong-import-position
# pylint: disable=ungrouped-imports,multiple-imports,reimported,no-name-in-module,no-member
# cspell:words isdst
# pylint: disable=missing-class-docstring,too-few-public-methods,unexpected-keyword-arg
import abc
import argparse
import asyncio
import calendar
import dis
import email.utils
import gettext
import hashlib
import importlib.util
import shutil
import sys
import sysconfig
import threading
import typing
import weakref
import xml.etree.ElementTree as ET
from abc import abstractproperty
from asyncio import sleep
from shutil import rmtree
from typing import no_type_check_decorator

# Keyword and positional arguments.
int(x=1)
bool(x=0)
float(x=1.5)
int("1")
int(1, base=2)
asyncio.sleep(1, loop=None)
sleep(1, loop=None)
asyncio.gather(loop=None)
asyncio.wait_for(None, 1, loop=None)
gettext.translation("d", None, None, None, None, "utf-8")
gettext.translation("d", codeset="utf-8")
gettext.translation("d", None, None, None, None)
gettext.install("d", None, "utf-8")
shutil.rmtree("p", False, print)
shutil.rmtree("p", onerror=print)
shutil.rmtree("p", False)
rmtree("p", False, print)
rmtree("p", ignore_errors=True)
argparse.BooleanOptionalAction("a", "b", None, None, None, None, None, None)
argparse.BooleanOptionalAction("a", "b", None, None)
argparse.BooleanOptionalAction("a", "b", type=int, metavar="m")
email.utils.localtime(None, True)
email.utils.localtime(None, isdst=1)
email.utils.localtime(None)
sysconfig.is_python_build(True)
sysconfig.is_python_build(check_home=True)
sysconfig.is_python_build()
dis.get_instructions(print, first_line=None, show_caches=True)
dis.get_instructions(print, None, True)
dis.get_instructions(print)
threading.RLock(1)
threading.RLock(x=1)
threading.RLock()
weakref.finalize(print, func=print)
weakref.finalize(print, print, obj=1)
weakref.finalize(print, print)
importlib.util.cache_from_source("p", True)
importlib.util.cache_from_source("p", debug_override=True)
importlib.util.cache_from_source("p")
hashlib.md5(string=b"x")
hashlib.new("md5", string=b"x")
hashlib.sha256(b"x")
shutil.rmtree(*[], **{})
shutil.rmtree("p", *[1, 2])
shutil.rmtree("p", **{"onerror": print})


def shadowed(int, shutil):
    int(x=1)
    shutil.rmtree("p", onerror=print)


class Own:
    def rmtree(self, path, onerror=None):
        return path, onerror


Own().rmtree("p", onerror=print)

# Decorators: only the first one counts.
@abc.abstractproperty
def prop_a(self):
    return 1


@abstractproperty
def prop_b(self):
    return 1


@abc.abstractclassmethod
def class_a(cls):
    return 1


@abc.abstractstaticmethod
def static_a():
    return 1


@no_type_check_decorator
def checked_a():
    return 1


@typing.no_type_check_decorator
def checked_b():
    return 1


@abc.abstractproperty
@property
def two_a(self):
    return 1


@property
@abc.abstractproperty
def two_b(self):
    return 1


@staticmethod
@typing.no_type_check_decorator
def two_c():
    return 1


class Decorated:
    @abc.abstractproperty
    def method(self):
        return 1

    @abc.abstractstaticmethod
    def static():
        return 1


@typing.no_type_check_decorator
class DecoratedClass:
    pass


@abc.abstractproperty()
def called():
    return 1


@asyncio.coroutine
def gone():
    return 1


@abc.abstractmethod
def fine():
    return 1


# Attributes.
sys.last_type
sys.last_value
sys.last_traceback
x = sys.last_type
sys.last_type = 1
del sys.last_value
calendar.January
calendar.February
calendar.March
dis.HAVE_ARGUMENT
typing.AnyStr
typing.Any
ET.VERSION
xml_version = ET.VERSION
sys.version_info
sys.maxsize
sys.last_type.__class__
print(sys.last_type, calendar.January)
getattr(sys, "last_type")


def inner(sys):
    return sys.last_type


def shadow():
    import sys as another

    return another.last_type, another.maxsize
