"""Imports seen through scopes: shadowing, locals, classes, deferred use."""
import shutil
import logging
import logging.handlers
import collections.abc
import json


def shadow(shutil):
    return shutil


def shadowed_by_local():
    logging = 1
    return logging


def local_imports():
    import base64
    from struct import pack, unpack
    return pack


def outer():
    import csv

    def inner():
        return csv

    return inner


class Holder:
    import pathlib

    attribute = collections.abc.Mapping


json = None
print(json)
