import os
import sys
import logging
import re
from collections import OrderedDict
from django.db import models
from django.utils.translation import gettext_lazy as _

a_int = int("5")
a_str = str(5)
a_float = float("1.5")
a_bool = bool(1)
a_bytes = bytes(3)
a_len = len("abc")
a_list = list()
a_dict = dict()
a_set = set()
a_tuple = tuple()
a_obj = object()
a_frozen = frozenset()
a_range = range(3)
a_mod = "a%s" % 1
a_env = os.environ["x"]
a_log = logging.getLogger("x")
a_re = re.compile("x")
a_od = OrderedDict()
a_ex = Exception("x")
a_none = None
a_tr = _("x")
a_abs = abs(-1)
a_max = max(1, 2)
a_sum = sum([1])
a_if = 1 if os.name else 2
a_neg = -len("a")
a_dictget = {}.get("x")
a_fstr = f"{1}"
a_fstr2 = f"{os.name}"
a_set2 = {1, 2}
a_list2 = [1] + [2]
a_star = [*range(3)]
a_cmp = 1 < 2 < 3
a_lam = (lambda: 1)
a_gen = (i for i in range(3))
a_comp = [i for i in range(3)]
a_dcomp = {i: i for i in range(3)}


class SessionMaker:
    pass


SessionMaker = SessionMaker()


CamelComp = [i for i in range(3)]
CamelGen = (i for i in range(3))


class Local:
    pass


class Maker:
    pass


def make():
    Alias = Local
    Other = Maker()
    return Alias, Other


from datetime import date
from enum import Enum, IntEnum


class Meta(type):
    pass


class SubMeta(Meta):
    pass


class Color(Enum):
    RED = 1


MetaMade = Meta("MetaMade", (object,), {})
metaMade = SubMeta("metaMade", (object,), {})
metaTwo = Meta("metaTwo", (object,))
enumMade = Enum("enumMade", "a b")
enumInt = IntEnum("enumInt", "a b")
enumSub = Color("enumSub", "a b")
pastDate = date
dictUnpack = {**unknown_thing()}
dictNested = {**{"a": 1}}


def functional():
    madeInFn = Meta("madeInFn", (object,), {})
    enumInFn = Enum("enumInFn", "a b")
    enumIntFn = IntEnum("enumIntFn", "a b")
    return madeInFn, enumInFn, enumIntFn
strUpper = "x".upper()
snake_join = ",".join(unknown_thing())
bytesDecode = b"x".decode()
snake_split = "x".split(",")
snake_unknown = unknown_thing().upper()
fmtDone = "{} {a}".format(1, a=2)
snake_fmt_missing = "{a}".format(**globals())
snake_fmt_args = "{}".format(unknown_thing())
stdSubscript = logging.StreamHandler[int]
