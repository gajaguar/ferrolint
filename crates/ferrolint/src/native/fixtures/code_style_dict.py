from typing import Final
import typing

# sub-dicts with a common string key
ONE = {
    "a": {"x": 1, "y": 2},
    "b": {"x": 3, "z": 4},
}

# no common key
TWO = {
    "a": {"x": 1},
    "b": {"y": 2},
}

# a single entry
THREE = {"a": {"x": 1}}

# a non-string key
FOUR = {
    "a": {1: 1},
    "b": {1: 2},
}

# a bytes key
FIVE = {
    "a": {b"x": 1},
    "b": {b"x": 2},
}

# a mixed value
SIX = {
    "a": {"x": 1},
    "b": [1],
}

# lists of one length
SEVEN = {
    "a": [1, 2],
    "b": [3, 4],
}

# tuples of one length
EIGHT = {
    "a": (1, 2),
    "b": (3, 4),
}

# lists and tuples
NINE = {
    "a": [1, 2],
    "b": (3, 4),
}

# lengths differ
TEN = {
    "a": [1, 2],
    "b": [3],
}

# empty lists
ELEVEN = {
    "a": [],
    "b": [],
}

# lists of dicts only
TWELVE = {
    "a": [{"x": 1}],
    "b": [{"y": 2}],
}

# lists of dicts and others
THIRTEEN = {
    "a": [{"x": 1}, 1],
    "b": [{"y": 2}, 2],
}

# a dict unpacking
FOURTEEN = {
    **ONE,
    "a": (1, 2),
}

# the keys are the same text in other quotes
FIFTEEN = {
    "a": {"x": 1},
    'b': {'x': 2},
}

# empty sub-dicts
SIXTEEN = {
    "a": {},
    "b": {},
}

# annotated assignments
SEVENTEEN: dict = {
    "a": (1, 2),
    "b": (3, 4),
}
EIGHTEEN: Final = {
    "a": (1, 2),
    "b": (3, 4),
}
NINETEEN: typing.Final = {
    "a": (1, 2),
    "b": (3, 4),
}
TWENTY: Final[dict] = {
    "a": (1, 2),
    "b": (3, 4),
}
TWENTY_ONE: "Final" = {
    "a": (1, 2),
    "b": (3, 4),
}
ANNOTATED_WITHOUT_VALUE: dict

# a chained assignment
A = B = {
    "a": (1, 2),
    "b": (3, 4),
}

# not a direct value
TWENTY_TWO = dict(
    a=(1, 2),
    b=(3, 4),
)
TWENTY_THREE = [
    {
        "a": (1, 2),
        "b": (3, 4),
    }
]
TWENTY_FOUR = {"k": {"a": (1, 2), "b": (3, 4)}}

if True:
    NESTED = {
        "a": (1, 2),
        "b": (3, 4),
    }
    NESTED_FINAL: Final = {
        "a": (1, 2),
        "b": (3, 4),
    }


def function():
    local = {
        "a": (1, 2),
        "b": (3, 4),
    }
    local_final: Final = {
        "a": (1, 2),
        "b": (3, 4),
    }
    return local, local_final


class Klass:
    attribute = {
        "a": (1, 2),
        "b": (3, 4),
    }
    attribute_final: Final = {
        "a": (1, 2),
        "b": (3, 4),
    }
    self_final: Final = {"a": (1, 2), "b": (3, 4)}
