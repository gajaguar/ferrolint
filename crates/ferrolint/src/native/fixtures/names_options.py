import enum
from typing import Final, TypeVar, ClassVar

foo = 1
bar = 2
CONST_VAL = 3
camelConst = 4
good_thing = 5
_tmp_x = 6
ok_name = 7
notgood = 8


def Func(camelArg, snake_arg, ok_name, UPPER):
    local_var = 1
    LOCAL = 2
    good_x = 3
    foo = 4
    return [item for item in range(3)]


def bad_func(a):
    pass


class snake_class:
    CLS_ATTR = 1
    cls_attr = 2
    FINAL: Final = 1
    final_c: Final = 2
    Camel = 3

    def METHOD(self):
        self.PascalAttr = 1
        self.snake_attr = 2

    def method(self):
        pass

    @property
    def Prop(self):
        return 1

    @property
    def prop_snake(self):
        return 2


class Color(enum.Enum):
    RED = 1
    green = 2


T = TypeVar("T")
a_b = TypeVar("a_b")
