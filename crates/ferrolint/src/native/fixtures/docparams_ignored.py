"""Fixture for the ignored parameters and the missing documentation of docparams."""


def sphinx_ignored(a, _b, unused_c, ignored_d):
    """Add.

    :param a: first
    :param _b: second
    :param unused_c: third
    :param ignored_d: fourth
    :type a: int
    :type _b: int
    :type ignored_d: int
    """


def sphinx_ignored_description_only(a, _b):
    """Add.

    :param a: first
    :param _b: second
    :type a: int
    """


def sphinx_ignored_annotated(a: int, _b: int):
    """Add.

    :param a: first
    """


def sphinx_ignored_type_only(a, _b):
    """Add.

    :param a: first
    :type a: int
    :type _b: int
    """


def sphinx_not_documented_ignored(a, _b):
    """Add.

    :param a: first
    :type a: int
    """


def google_ignored(a, _b):
    """Add.

    Args:
        a (int): first
        _b (int): second
    """


def numpy_ignored(a, _b):
    """Add.

    Parameters
    ----------
    a : int
        first
    _b : int
        second
    """


def nothing_documented(a, b):
    """Add.

    :returns: the sum
    """


def nothing_documented_google(a, b):
    """Add.

    Returns:
        the sum
    """


def nothing_documented_annotated(a: int, b):
    """Add.

    :returns: the sum
    """


def nothing_documented_ignored(_a, _b):
    """Add.

    :returns: the sum
    """


def nothing_documented_variadic(*args, **kwargs):
    """Add.

    :returns: the sum
    """


def nothing_documented_method(self, a):
    """Add.

    :returns: the sum
    """


def only_self(self):
    """Add.

    :returns: the sum
    """


def no_parameters():
    """Add.

    :returns: the sum
    """


class Shape:
    """A shape.

    :returns: nothing
    """

    def __init__(self, a, b):
        """Create.

        :returns: nothing
        """

    def method(self, a, b):
        """Add.

        :returns: the sum
        """


class Documented:
    """A documented class.

    :param a: first
    :type a: int
    """

    def __init__(self, a, _b):
        """Create."""
