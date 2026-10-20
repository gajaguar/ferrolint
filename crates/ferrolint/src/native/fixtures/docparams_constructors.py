"""Fixture for the constructor messages of pylint.extensions.docparams."""


class Both:
    """Both.

    :param a: first
    :type a: int
    """

    def __init__(self, a):
        """Init.

        :param a: first
        :type a: int
        """


class OnlyClass:
    """Only the class documents.

    :param a: first
    :type a: int
    :param b: second
    :type b: int
    """

    def __init__(self, a, b):
        """Init."""


class OnlyInit:
    """Only init."""

    def __init__(self, a, b):
        """Init.

        :param a: first
        :type a: int
        """


class ClassMissing:
    """Class.

    :param a: first
    :type a: int
    """

    def __init__(self, a, b):
        """Init text."""


class ClassDiffering:
    """Class.

    :param z: first
    :type z: int
    """

    def __init__(self, a):
        """Init text."""


class New:
    """New.

    Args:
        a (int): first
    """

    def __new__(cls, a, b):
        """New text."""


class GoogleBoth:
    """Both.

    Args:
        a (int): first
    """

    def __init__(self, a):
        """Init.

        Args:
            a (int): first
        """


class SeeInit:
    """See the constructor.

    For the parameters, see :meth:`__init__`.
    """

    def __init__(self, a, b):
        """Init.

        :param a: first
        """


class Nested:
    def method(self):
        class Inner:
            """Inner.

            :param a: first
            """

            def __init__(self, a, b):
                """Init.

                :param a: first
                """

        return Inner
