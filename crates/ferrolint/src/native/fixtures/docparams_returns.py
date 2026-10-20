"""Fixture for the return and yield messages of pylint.extensions.docparams."""
import abc
import functools


def returns_documented():
    """Return.

    :returns: a number
    :rtype: int
    """
    return 1


def returns_undocumented():
    """Return."""
    return 1


def returns_undocumented_twice(flag):
    """Return."""
    if flag:
        return 1
    return 2


def returns_none():
    """Return nothing."""
    return None


def returns_none_documented():
    """Return nothing.

    :returns: nothing
    """
    return None


def returns_documented_without_return():
    """Do something.

    :returns: nothing really
    :rtype: None
    """
    print("x")


def only_rtype_without_return():
    """Do something.

    :rtype: int
    """


def returns_in_nested_function():
    """Do something.

    :returns: a function
    """

    def inner():
        return 1

    return inner


def nested_return_only():
    """Do something.

    :returns: nothing
    """

    def inner():
        """Inner."""
        return 1

    inner()


def google_returns_ok():
    """Return.

    Returns:
        int: a number
    """
    return 1


def google_returns_missing():
    """Return.

    Args:
        none: unused
    """
    return 1


def google_returns_redundant():
    """Do something.

    Returns:
        int: a number
    """
    print("x")


def numpy_returns_ok():
    """Return.

    Returns
    -------
    int
        a number
    """
    return 1


def numpy_returns_redundant():
    """Do something.

    Returns
    -------
    int
        a number
    """


def numpy_yields_redundant():
    """Do something.

    Yields
    ------
    int
        a number
    """
    return 1


def numpy_yields_ok():
    """Generate.

    Yields
    ------
    int
        a number
    """
    yield 1


def yields_undocumented():
    """Generate."""
    yield 1


def yields_undocumented_from():
    """Generate."""
    yield from [1, 2]


def yields_sphinx_returns():
    """Generate.

    :returns: numbers
    :rtype: int
    """
    yield 1


def yields_google_documented():
    """Generate.

    Yields:
        int: numbers
    """
    yield 1


def yields_google_with_returns():
    """Generate.

    Returns:
        int: numbers
    """
    yield 1


def yields_google_redundant():
    """Do something.

    Yields:
        int: numbers
    """
    return 1


def yields_sphinx_redundant():
    """Do something.

    :yields: numbers
    :ytype: int
    """
    return 1


def generator_with_returns_doc():
    """Generate.

    :returns: numbers
    :rtype: int
    """
    yield 1
    return 2


def yield_in_nested_function():
    """Do something.

    Yields:
        int: numbers
    """

    def inner():
        yield 1

    return inner


class Shape:
    """A shape."""

    @property
    def area(self):
        """The area."""
        return 1

    @property
    def sphinx_area(self):
        """The area.

        :type: int
        """
        return 1

    @property
    def google_area(self):
        """int: The area."""
        return 1

    @functools.cached_property
    def cached(self):
        """The cached value."""
        return 1

    @abc.abstractmethod
    def abstract_one(self):
        """Do something.

        :returns: a number
        """
        return 1

    @abc.abstractmethod
    def abstract_generator(self):
        """Do something.

        :returns: a number
        """
        yield 1

    def pass_only(self):
        """Do something.

        :returns: a number
        """
        pass

    def not_implemented(self):
        """Do something.

        :returns: a number
        """
        raise NotImplementedError

    def raises_other(self):
        """Do something.

        :returns: a number
        """
        raise ValueError("no")

    def docstring_only(self):
        """Do something.

        :returns: a number
        """

    def _private(self):
        return 1

    async def async_one(self):
        """Do something."""
        return 1

    async def async_generator(self):
        """Do something."""
        yield 1


def no_docstring_at_all():
    return 1


def empty_docstring():
    """"""
    return 1
