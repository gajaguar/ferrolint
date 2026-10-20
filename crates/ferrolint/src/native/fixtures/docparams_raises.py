"""Fixture for the raise messages of pylint.extensions.docparams."""
import contextlib
import re
import socket
from contextlib import suppress


class AppError(Exception):
    """Base error."""


class ConfigError(AppError):
    """Bad configuration."""


class Unrelated:
    """Not an exception."""


def sphinx_documented():
    """Do something.

    :raises ValueError: always
    """
    raise ValueError("no")


def sphinx_undocumented_section():
    """Do something.

    :returns: nothing
    """
    raise ValueError("no")


def sphinx_wrong_exception():
    """Do something.

    :raises TypeError: never
    """
    raise ValueError("no")


def sphinx_two_raised():
    """Do something.

    :raises ValueError: sometimes
    """
    if input():
        raise ValueError("no")
    raise KeyError("no")


def sphinx_both_documented():
    """Do something.

    :raises ValueError: sometimes
    :raises KeyError: sometimes
    """
    if input():
        raise ValueError("no")
    raise KeyError("no")


def sphinx_comma_separated():
    """Do something.

    :raises ValueError, KeyError: sometimes
    """
    if input():
        raise ValueError("no")
    raise KeyError("no")


def sphinx_qualified_name():
    """Do something.

    :raises builtins.ValueError: always
    """
    raise ValueError("no")


def no_sections_but_text():
    """Do something and fail."""
    raise ValueError("no")


def ancestor_documented():
    """Do something.

    :raises Exception: always
    """
    raise ValueError("no")


def ancestor_documented_oserror():
    """Do something.

    :raises OSError: always
    """
    raise FileNotFoundError("no")


def ancestor_not_documented():
    """Do something.

    :raises ArithmeticError: always
    """
    raise KeyError("no")


def alias_ioerror():
    """Do something.

    :raises ValueError: never
    """
    raise IOError("no")


def alias_socket_error():
    """Do something.

    :raises ValueError: never
    """
    raise socket.error("no")


def local_class_documented():
    """Do something.

    :raises AppError: always
    """
    raise ConfigError("no")


def local_class_undocumented():
    """Do something.

    :raises ValueError: never
    """
    raise ConfigError("no")


def local_class_builtin_ancestor():
    """Do something.

    :raises Exception: always
    """
    raise ConfigError("no")


def not_an_exception_class():
    """Do something.

    :raises ValueError: never
    """
    raise Unrelated()


def raise_class_not_call():
    """Do something.

    :raises ValueError: never
    """
    raise KeyError


def raise_from():
    """Do something.

    :raises ValueError: never
    """
    try:
        pass
    except ValueError as error:
        raise KeyError("no") from error


def bare_reraise():
    """Do something.

    :raises TypeError: never
    """
    try:
        pass
    except ValueError:
        raise


def bare_reraise_tuple():
    """Do something.

    :raises TypeError: never
    """
    try:
        pass
    except (ValueError, KeyError):
        raise


def bare_reraise_documented():
    """Do something.

    :raises ValueError: sometimes
    """
    try:
        pass
    except ValueError:
        raise


def bare_reraise_without_type():
    """Do something.

    :raises TypeError: never
    """
    try:
        pass
    except:  # noqa
        raise


def bare_raise_outside_handler():
    """Do something.

    :raises TypeError: never
    """
    raise


def caught_by_name():
    """Do something.

    :raises TypeError: never
    """
    try:
        raise ValueError("no")
    except ValueError:
        pass


def caught_by_tuple():
    """Do something.

    :raises TypeError: never
    """
    try:
        raise ValueError("no")
    except (KeyError, ValueError):
        pass


def caught_by_base_class():
    """Do something.

    :raises TypeError: never
    """
    try:
        raise ValueError("no")
    except Exception:
        pass


def caught_by_bare_except():
    """Do something.

    :raises TypeError: never
    """
    try:
        raise ValueError("no")
    except:  # noqa
        pass


def caught_in_else():
    """Do something.

    :raises TypeError: never
    """
    try:
        pass
    except ValueError:
        pass
    else:
        raise ValueError("no")


def caught_in_finally():
    """Do something.

    :raises TypeError: never
    """
    try:
        pass
    except ValueError:
        pass
    finally:
        raise ValueError("no")


def raised_in_handler_is_not_caught():
    """Do something.

    :raises TypeError: never
    """
    try:
        pass
    except ValueError:
        raise ValueError("no")


def raised_in_nested_try_of_handler():
    """Do something.

    :raises TypeError: never
    """
    try:
        pass
    except KeyError:
        try:
            raise ValueError("no")
        except ValueError:
            pass


def raised_in_nested_function_of_try():
    """Do something.

    :raises TypeError: never
    """
    try:

        def inner():
            raise ValueError("no")

    except ValueError:
        pass


def suppressed():
    """Do something.

    :raises TypeError: never
    """
    with contextlib.suppress(ValueError):
        raise ValueError("no")


def suppressed_tuple():
    """Do something.

    :raises TypeError: never
    """
    with suppress(KeyError, ValueError):
        raise ValueError("no")


def suppressed_other():
    """Do something.

    :raises TypeError: never
    """
    with suppress(KeyError):
        raise ValueError("no")


async def suppressed_async():
    """Do something.

    :raises TypeError: never
    """
    async with suppress(ValueError):
        raise ValueError("no")


def regex_error():
    """Do something.

    :raises re.error: always
    """
    raise re.error("no")


def google_documented():
    """Do something.

    Raises:
        ValueError: always
    """
    raise ValueError("no")


def google_undocumented():
    """Do something.

    Raises:
        TypeError: never
    """
    raise ValueError("no")


def google_missing_section():
    """Do something.

    Returns:
        int: never
    """
    raise ValueError("no")


def numpy_documented():
    """Do something.

    Raises
    ------
    ValueError
        always
    """
    raise ValueError("no")


def numpy_undocumented():
    """Do something.

    Raises
    ------
    TypeError
        never
    """
    raise ValueError("no")


def twice_the_same():
    """Do something.

    :raises TypeError: never
    """
    if input():
        raise ValueError("no")
    raise ValueError("again")


def nested_function_raises():
    """Do something.

    :raises TypeError: never
    """

    def inner():
        """Inner.

        :raises TypeError: never
        """
        raise ValueError("no")

    inner()


def raise_in_class_body():
    """Do something.

    :raises TypeError: never
    """

    class Inner:
        value = 1

    return Inner


def raise_not_implemented():
    """Do something.

    :raises TypeError: never
    """
    raise NotImplementedError


def not_implemented_with_other():
    """Do something.

    :raises TypeError: never
    """
    if input():
        raise NotImplementedError
    raise ValueError("no")


def no_docstring():
    raise ValueError("no")


def _private_one():
    """Do something.

    :raises TypeError: never
    """
    raise ValueError("no")


def empty_docstring():
    """"""
    raise ValueError("no")


class Item:
    """An item."""

    @property
    def value(self):
        """The value.

        :raises TypeError: never
        """
        return 1

    @value.setter
    def value(self, new):
        raise ValueError("no")

    @property
    def other(self):
        """The other.

        :raises ValueError: when set
        """
        return 1

    @other.setter
    def other(self, new):
        raise ValueError("no")

    @property
    def plain(self):
        """The plain."""
        return 1

    @plain.setter
    def plain(self, new):
        raise ValueError("no")

    @property
    def third(self):
        """The third.

        :raises TypeError: never
        """
        return 1

    @third.setter
    def third(self, new):
        """Set it."""
        raise ValueError("no")

    def method(self):
        """Do something.

        :raises TypeError: never
        """
        raise NotImplementedError

    def method_two(self):
        """Do something.

        :raises TypeError: never
        """
        raise NotImplementedError("no")

    def method_with_body(self):
        """Do something.

        :raises TypeError: never
        """
        print("x")
        raise NotImplementedError


class Registry:
    """A registry."""

    class Missing(KeyError):
        """Nested error."""

    Failure = ConfigError
    Plain = ValueError
    Bad = Unrelated

    def nested_class(self):
        """Do something.

        :raises ValueError: never
        """
        raise self.Missing("no")

    def nested_class_ancestor(self):
        """Do something.

        :raises LookupError: always
        """
        raise self.Missing("no")

    def assigned_class(self):
        """Do something.

        :raises ValueError: never
        """
        raise self.Failure("no")

    def assigned_builtin(self):
        """Do something.

        :raises TypeError: never
        """
        raise self.Plain("no")

    @classmethod
    def from_class(cls):
        """Do something.

        :raises TypeError: never
        """
        raise cls.Plain("no")

    def unknown_attribute(self):
        """Do something.

        :raises TypeError: never
        """
        raise self.Other("no")

    def other_object(self, other):
        """Do something.

        :raises TypeError: never
        """
        raise other.Plain("no")

    def not_called(self):
        """Do something.

        :raises TypeError: never
        """
        raise self.Plain

    def handler_attribute(self):
        """Do something.

        :raises TypeError: never
        """
        try:
            pass
        except self.Plain:
            raise

    def suppressed_attribute(self):
        """Do something.

        :raises TypeError: never
        """
        with suppress(self.Plain):
            raise self.Plain("no")
