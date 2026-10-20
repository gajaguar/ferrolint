"""Fixture for docstring-min-length and default-docstring-type."""


def short(a, b):
    """Add.

    Args:
        a (int): first
    """


def long(a, b):
    """Add.

    Args:
        a (int): first
    """
    c = a + b
    if c:
        c += 1
    for _ in range(3):
        c += 1
    return c


def decorated(a, b):
    """Add.

    Args:
        a (int): first
    """
    try:
        pass
    finally:
        c = 1
        d = 2
        e = 3


def untyped_style(a, b):
    """Add.

    a: first
    """
