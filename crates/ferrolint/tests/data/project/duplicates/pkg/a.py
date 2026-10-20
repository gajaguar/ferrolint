"""Module a."""
import os
from collections import OrderedDict


def compute(values, scale=2):
    """Total of the values."""
    total = 0
    for value in values:
        total += value * scale
        total -= 1
    return total


@staticmethod
def decorated(
    first,
    second,
):
    result = first + second
    result *= 2
    result -= 3
    result += 4
    return result


def unique_a():
    return os, OrderedDict
