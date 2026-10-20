"""Fixture for the type comments that pylint.extensions.docparams reads."""


def own_line(a, b):
    # type: (int, str) -> None
    """Do something.

    :param a: first
    :param b: second
    """


def same_line(a, b):  # type: (int, str) -> None
    """Do something.

    :param a: first
    :param b: second
    """


def blank_line_before(a, b):

    # type: (int, str) -> None
    """Do something.

    :param a: first
    :param b: second
    """


def partly(a, b):
    # type: (int) -> None
    """Do something.

    :param a: first
    :param b: second
    """


def ellipsis_types(a, b):
    # type: (...) -> None
    """Do something.

    :param a: first
    :param b: second
    """


def ellipsis_one(a, b):
    # type: (..., int) -> None
    """Do something.

    :param a: first
    :param b: second
    """


def nested_types(a, b):
    # type: (Dict[str, List[int]], Tuple[int, str]) -> None
    """Do something.

    :param a: first
    :param b: second
    """


def quoted_types(a, b):
    # type: ("int, str", 'a, b') -> None
    """Do something.

    :param a: first
    :param b: second
    """


def trailing_comment(a, b):
    # type: (int, str) -> None  # not important
    """Do something.

    :param a: first
    :param b: second
    """


def star_args(a, *args, **kwargs):
    # type: (int, *str, **str) -> None
    """Do something.

    :param a: first
    :param args: more
    :param kwargs: more
    """


def kwonly(a, *, b):
    # type: (int, str) -> None
    """Do something.

    :param a: first
    :param b: second
    """


def ignored_comment(a, b):
    # type: ignore
    """Do something.

    :param a: first
    :param b: second
    """


def not_a_function_type(a, b):
    # type: int
    """Do something.

    :param a: first
    :param b: second
    """


def no_return_part(a, b):
    # type: (int, str)
    """Do something.

    :param a: first
    :param b: second
    """


def annotated_wins(a: int, b):
    # type: (str, str) -> None
    """Do something.

    :param a: first
    :param b: second
    """


class Thing:
    """A thing."""

    def method(self, a, b):
        # type: (int, str) -> None
        """Do something.

        :param a: first
        :param b: second
        """

    def method_without_self_type(self, a, b):
        # type: (self, int, str) -> None
        """Do something.

        :param a: first
        :param b: second
        """

    @classmethod
    def build(cls, a):
        # type: (int) -> Thing
        """Do something.

        :param a: first
        """


def per_parameter(
    a,  # type: int
    b,  # type: str
):
    # type: (...) -> None
    """Do something.

    :param a: first
    :param b: second
    """


def per_parameter_partly(
    a,  # type: int
    b,
    c=1,  # type: int
):
    # type: (...) -> None
    """Do something.

    :param a: first
    :param b: second
    :param c: third
    """


def per_parameter_no_trailing_comma(
    a,  # type: int
    b  # type: str
):
    # type: (...) -> None
    """Do something.

    :param a: first
    :param b: second
    """


def per_parameter_same_line(a, b):  # not a type comment
    """Do something.

    :param a: first
    :param b: second
    """


def per_parameter_with_ignore(
    a,  # type: ignore
    b,  # type: ...
):
    """Do something.

    :param a: first
    :param b: second
    """


def per_parameter_keyword_only(
    a,  # type: int
    *,
    b,  # type: str
):
    """Do something.

    :param a: first
    :param b: second
    """


def per_parameter_vararg(
    a,  # type: int
    *args  # type: str
):
    """Do something.

    :param a: first
    :param args: more
    """
