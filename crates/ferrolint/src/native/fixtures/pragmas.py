"""Pragmas whose informational messages Pylint reports."""
# pylint: disable=W0611
import os
import sys  # pylint: disable=unused-import
X = 1  # pylint: disable-next=W0612
Y = 2
Z = 3  # pylint: disable-msg=invalid-name
# pylint: enable-msg=W0612
# pylint: enable=W0611, unused-import
# pylint: disable=w0611
# pylint: disable=nonexistent
# pylint: disable=too-many-lines
# pylint: disable=F
# pylint: disable=imports
# pylint: disable=missing-docstring
# pylint: disable=W0703
# pylint: bogus=1
# pylint: disable
# pylint: disable=
# pylint: disable W0611
# pylint: skip-file=3
# pylint: foo
# pylint: =
# pylint: disable=W0611; enable=W0612
# pylint: disable=W0611 # pylint: disable=W0612
# pylint: disable = W0611, W0612
# pylint: disable-next=W0611
IMPORTED = 4
# pylint: disable=duplicate-code,R0801
# pylint: disable=W0611 skip-file=3
# pylint: disable=W0611 enable
# pylint: enable=C0114
# pylint: disable-next=C0114,wrong-import-order
# note # pylint: disable=W0612 # trailing
# xpylint: disable=W0612
# pylint:: disable=W0612
# pylint: disable=W0511, bad-inline-option
# pylint: disable
