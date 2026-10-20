"""Import under a disable of the function scope."""


def load():
    # pylint: disable=cyclic-import
    from top import z2

    return z2
