"""Module-level imports, used and unused."""
from __future__ import annotations

import os
import os.path
import sys as system
import json.decoder as jd
from collections import OrderedDict
from collections import defaultdict as dd
from typing import (
    Any,
    List,
)
import re, string
import textwrap
import gettext as _
import a.b
import a.c

print(List, re, a.c)


def uses_textwrap():
    return textwrap.dedent("x")
