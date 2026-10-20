"""isinstance-second-argument-not-valid-type."""
from typing import Optional


class Local:
    pass


def func():
    pass


LITERAL = 1
CLASSES = (int, str)
BAD_TUPLE = (int, "str")
UNION = int | str
BAD_UNION = int | 3
INSTANCE = Local()
NAME_ALIAS = LITERAL


def checks(value, kind, *rest):
    isinstance(value, 1)  # flagged
    isinstance(value, "str")  # flagged
    isinstance(value, None)  # flagged
    isinstance(value, [int, str])  # flagged
    isinstance(value, {int: 1})  # flagged
    isinstance(value, {int})  # flagged
    isinstance(value, (int, "a"))  # flagged
    isinstance(value, (int, (str, 2)))  # flagged
    isinstance(value, int | 1)  # flagged
    isinstance(value, 1 | int)  # flagged
    isinstance(value, int | None)
    isinstance(value, None | int)
    isinstance(value, func)  # flagged
    isinstance(value, len)  # flagged
    isinstance(value, lambda: 1)  # flagged
    isinstance(value, LITERAL)  # flagged
    isinstance(value, NAME_ALIAS)  # flagged
    isinstance(value, BAD_TUPLE)  # flagged
    isinstance(value, BAD_UNION)
    isinstance(value, INSTANCE)  # flagged
    isinstance(value, f"{kind}")
    isinstance(value, f"plain")  # flagged
    isinstance(value, int)
    isinstance(value, (int, str))
    isinstance(value, ((int, str), float))
    isinstance(value, Local)
    isinstance(value, CLASSES)
    isinstance(value, UNION)
    isinstance(value, kind)
    isinstance(value, (kind, int))
    isinstance(value, *rest)
    isinstance(value, Optional[int])
    isinstance(value, list[int])
    isinstance(value, type(value))
    isinstance(value)
    isinstance(value, 1, 2)  # flagged
    isinstance(value, second=1)


def shadowed(isinstance):
    isinstance(1, 2)
