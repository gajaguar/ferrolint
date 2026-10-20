"""W4901, W4902 and W4904 against Pylint 4.1.3 on Python 3.14."""
# pylint: disable=unused-import,import-error,missing-function-docstring,wrong-import-position
# pylint: disable=ungrouped-imports,multiple-imports,reimported,no-name-in-module,no-member
import imp
import distutils.core
import os, telnetlib
import xml.etree.cElementTree as ET
import importlib
import impl
import typing.io
import typing.re as typing_re
from distutils import core
from distutils.command import build
from asyncore import dispatcher
from . import imp as relative_imp
from .distutils import core as relative_core
from tkinter import tix
import tkinter.tix
from profile import Profile
import cProfile

__import__("imp")
__import__("importlib")
__import__("imp", {})
__import__(name)
mod = __import__("telnetlib")


def late():
    import cgi
    from cgitb import enable


# Deprecated names that astroid infers on this Python.
import logging
import logging as lg
import threading
import codecs
import platform
import sys
import sysconfig
import inspect
from logging import warn
from threading import currentThread as current

logging.warn("x")
lg.warn("x")
warn("x")
threading.currentThread()
threading.activeCount()
current()
codecs.open("f")
platform.java_ver()
sys._clear_type_cache()
sysconfig.expand_makefile_vars("x", {})
inspect.getcallargs(print)
logging.warning("x")


# Names Python 3.14 has removed are not inferred, so Pylint is silent.
import time
import asyncio

time.clock()
asyncio.coroutine(print)
inspect.getargspec(print)


# A function of the file or a builtin named like a deprecated bare name.
def assert_(value):
    return value


def local_names(logging):
    logging.warn("x")
    assert_(1)
    return len(logging.warn)


class Plain:
    def warn(self):
        return self.warn()


Plain().warn()


# Deprecated classes, named by the text of the owner.
import collections
import collections.abc
import ast
import asyncio
import argparse
import configparser
import configparser.SafeConfigParser
import glob
from collections import Mapping, MutableMapping, OrderedDict
from collections.abc import ByteString, Mapping as Mapping2
from ast import Num, Str, Constant
from asyncio import AbstractEventLoopPolicy as Policy, run
from argparse import FileType, ArgumentParser
from configparser import SafeConfigParser, ConfigParser
from typing import Text, ByteString as TypingBytes
from webbrowser import MacOSX
from glob import glob0, glob
from smtpd import MailmanProxy
from importlib.abc import Finder, Loader
from pkgutil import ImpImporter
from . import Mapping as RelativeMapping
from .collections import Iterable

if sys.version_info >= (3, 0):
    from collections import Iterable
else:
    from collections import Sized

if sys.version_info[0] == 3:
    from collections import Callable
elif sys.version_info[0] == 2:
    from collections import Container
else:
    from collections import Hashable

if sys.platform == "linux":
    from collections import Reversible

if True:
    from collections import Awaitable

collections.Mapping()
collections.OrderedDict()
ast.Num(1)
ast.Constant(1)
asyncio.AbstractEventLoopPolicy()
argparse.FileType("r")
configparser.SafeConfigParser()
configparser.ConfigParser()
other = collections
other.Mapping()
collections.abc.ByteString()
collections().Mapping()
collections.Mapping.register(int)
glob.glob0("x")
