"""Pragmas whose own messages a pragma disables."""
# pylint: disable=W0611
# pylint: disable=locally-disabled
# pylint: disable=W0612
# pylint: disable-next=W0613
import os


def first():
    """Inside a block."""
    # pylint: disable=I0023
    # pylint: disable=W0611
    # pylint: disable=bad-inline-option
    # pylint: disable
    return 1


# pylint: disable=W0613
# pylint: disable
