"""Fixture for the parameter messages of pylint.extensions.docparams (defaults)."""
from typing import overload


def sphinx_ok(a, b):
    """Add.

    :param a: first
    :param b: second
    :type a: int
    :type b: int
    """


def sphinx_missing(a, b, c):
    """Add.

    :param a: first
    :type a: int
    """


def sphinx_inline_types(a, b):
    """Add.

    :param int a: first
    :param str b: second
    """


def sphinx_differing(a, b):
    """Add.

    :param a: first
    :param z: renamed
    :type a: int
    :type y: int
    """


def sphinx_star(a, *args, **kwargs):
    """Add.

    :param a: first
    :param \*args: rest
    :param \**kwargs: more
    :type a: int
    """


def sphinx_star_bare(a, *args, **kwargs):
    """Add.

    :param a: first
    :param args: rest
    :param kwargs: more
    :type a: int
    """


def sphinx_all_missing(a, b):
    """Add.

    :returns: nothing
    """


def sphinx_nothing_documented(a, b):
    """Add numbers."""


def sphinx_see(a, b):
    """Add.

    For the parameters, see :func:`other`.

    :param a: first
    """


def sphinx_keyword(a, **kwargs):
    """Add.

    :keyword a: first
    :key b: other
    """


def annotated(a: int, b: str, c):
    """Add.

    :param a: first
    :param b: second
    :param c: third
    """


def annotated_all(a: int, b: str):
    """Add.

    :param a: first
    """


def posonly(a: int, /, b: str):
    """Add.

    :param a: first
    :param b: second
    """


def posonly_typed(a: int, /, b):
    """Add.

    :param a: first
    :param b: second
    """


def kwonly(*, a: int, b):
    """Add.

    :param a: first
    :param b: second
    """


def varargs_typed(*args: int, **kwargs: str):
    """Add.

    :param args: first
    :param kwargs: second
    """


def ellipsis_annotation(a: ..., b):
    """Add.

    :param a: first
    :param b: second
    """


def google_ok(a, b):
    """Add.

    Args:
        a (int): first
        b (str): second
    """


def google_missing(a, b, c):
    """Add.

    Args:
        a (int): first
        b: second
    """


def google_no_desc(a, b):
    """Add.

    Args:
        a (int):
        b (str): second
    """


def google_differing(a):
    """Add.

    Args:
        a (int): first
        z (int): renamed
    """


def google_continuation(a, b):
    """Add.

    Args:
        a (int): first
            continues here
        b (str): second
    """


def google_star(a, *args, **kwargs):
    """Add.

    Args:
        a (int): first
        *args (int): rest
        **kwargs (str): more
    """


def google_star_missing(a, *args, **kwargs):
    """Add.

    Arguments:
        a (int): first
    """


def google_keyword(a, **kwargs):
    """Add.

    Args:
        a (int): first

    Keyword Args:
        key (str): other
    """


def google_optional(a, b):
    """Add.

    Args:
        a (int, optional): first
        b (list[int] or None): second
    """


def google_all_missing(a, b):
    """Add.

    Returns:
        int: the sum
    """


def numpy_ok(a, b):
    """Add.

    Parameters
    ----------
    a : int
        first
    b : str
        second
    """


def numpy_missing(a, b, c):
    """Add.

    Parameters
    ----------
    a : int
        first
    b
        second
    """


def numpy_differing(a):
    """Add.

    Parameters
    ----------
    a : int
        first
    z : int
        renamed
    """


def numpy_multiple(a, b, c):
    """Add.

    Parameters
    ----------
    a, b : int
        the operands
    c
        third
    """


def numpy_type_only(a, b):
    """Add.

    Parameters
    ----------
    a : int
    b
    """


def numpy_star(a, *args, **kwargs):
    """Add.

    Parameters
    ----------
    a : int
        first
    *args : int
        rest
    **kwargs : str
        more
    """


def numpy_next_section(a, b):
    """Add.

    Parameters
    ----------
    a : int
        first

    Returns
    -------
    int
        total
    """


class Methods:
    def method(self, a, b):
        """Add.

        :param a: first
        :type a: int
        """

    @classmethod
    def build(cls, a):
        """Add.

        :param a: first
        :param z: renamed
        """

    @staticmethod
    def helper(a, b):
        """Add.

        :param a: first
        :type a: int
        :type b: int
        """

    def _private(self, a, b):
        """Skipped by the default no-docstring-rgx.

        :param a: first
        """

    def __init__(self, a, b):
        """Skipped by the default no-docstring-rgx.

        :param a: first
        """

    @overload
    def over(self, a: int) -> int:
        """Overload stub.

        :param z: wrong
        """

    def over(self, a):
        """Add.

        :param a: first
        :type a: int
        """

    def no_doc(self, a, b):
        pass

    def empty_doc(self, a, b):
        """"""


async def async_missing(a, b):
    """Add.

    :param a: first
    :type a: int
    """


def outer(a):
    """Outer.

    :param a: first
    :type a: int
    """

    def inner(b, c):
        """Inner.

        :param b: first
        :type b: int
        """

    return inner


def mixed(a, b):
    """Add.

    :param a: first
    :type a: int

    Args:
        b (int): second
    """


def default_ignored(a, _b, ignored_c, unused_d):
    """Add.

    :param a: first
    :type a: int
    """


def no_args():
    """Nothing.

    :param z: stray
    """


def plain_text(a, b):
    """Just text about a and b."""
