import os
from datetime import datetime

import swapper
from other import IMPORTED, helper

LITERAL = "x"

copyright = f"{datetime.now().year}, x"
epub_copyright = copyright
page_model_name = swapper.split(swapper.get_model_name("a", "Page"))[1]
parent_rel_name = f"{page_model_name.lower()}_ptr"
add_user_perm = f"{IMPORTED}.add_{IMPORTED.lower()}"
call_format = f"{helper()}"
lit_plain = f"abc"
lit_format = f"abc{LITERAL}"
lit_number = f"abc{1}"
env_name = f"{os.sep}x"

if x := os.environ.get("A"):
    backend = f"wagtail{x}"
else:
    backend = "plain"

SET_AUG = {"a"}
if IMPORTED:
    SET_AUG |= {"c"}

DICT_AUG = {"a": 1}
DICT_AUG |= {"b": 2}

TUPLE_AUG = (1,)
TUPLE_AUG += (2,)

list_aug = [1]
list_aug += [2]
