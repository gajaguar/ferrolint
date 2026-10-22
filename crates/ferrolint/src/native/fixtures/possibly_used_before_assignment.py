import sys

FLAG = input()


def one_branch(flag):
    if flag:
        value = 1
    return value


def both_branches(flag):
    if flag:
        value = 1
    else:
        value = 2
    return value


def elif_chain(a, b):
    if a:
        value = 1
    elif b:
        value = 2
    return value


def elif_else_chain(a, b):
    if a:
        value = 1
    elif b:
        value = 2
    else:
        value = 3
    return value


def else_raises(flag):
    if flag:
        value = 1
    else:
        raise ValueError
    return value


def else_returns(flag):
    if flag:
        value = 1
    else:
        return None
    return value


def body_returns(flag):
    if flag:
        return None
    else:
        value = 1
    return value


def else_exits(flag):
    if flag:
        value = 1
    else:
        sys.exit(1)
    return value


def same_test(flag):
    if flag:
        value = 1
    if flag:
        return value
    return None


def same_test_nested(flag, other):
    if flag:
        value = 1
    if other:
        if flag:
            return value
    return None


def different_test(flag, other):
    if flag:
        value = 1
    if other:
        return value
    return None


def nested_ifs(a, b):
    if a:
        if b:
            value = 1
        else:
            value = 2
    return value


def nested_complete(a, b):
    if a:
        if b:
            value = 1
        else:
            value = 2
    else:
        value = 3
    return value


def use_inside(flag):
    if flag:
        value = 1
        return value
    return None


def loop(items):
    if items:
        for item in items:
            result = item
    return result


def always_false(flag):
    if False:
        value = 1
    return value


def type_checking_only():
    from typing import TYPE_CHECKING

    if TYPE_CHECKING:
        from collections import OrderedDict
    return OrderedDict


def imports(flag):
    if flag:
        import json
        from os import path
    return json, path


def definitions(flag):
    if flag:
        def helper():
            return 1

        class Klass:
            pass
    return helper, Klass


def with_and_for(flag, items):
    if flag:
        with open("f") as stream:
            data = stream.read()
        for item in items:
            last = item
    return stream, data, last


def try_branch(flag):
    if flag:
        try:
            value = int(flag)
        except ValueError:
            value = 0
    return value


def try_partial(flag):
    if flag:
        try:
            value = int(flag)
        except ValueError:
            pass
    return value


def first_use_only(flag):
    if flag:
        value = 1
    print(value)
    return value


def augmented(flag):
    if flag:
        value = 1
    value += 1
    return value


def walrus(flag):
    if (value := flag):
        pass
    return value


def walrus_in_body(flag):
    if flag:
        if (value := flag):
            pass
    return value


def module_flag():
    return later


if FLAG:
    later = 1


def comprehension_use(flag, items):
    if flag:
        value = 1
    return [value for _ in items]


def lambda_use(flag):
    if flag:
        value = 1
    return lambda: value


def nested_function_use(flag):
    if flag:
        value = 1

    def inner():
        return value

    return inner


def else_only(flag):
    if flag:
        pass
    else:
        value = 1
    return value


def elif_only(a, b):
    if a:
        pass
    elif b:
        value = 1
    return value


def while_loop(flag):
    while flag:
        if flag:
            value = 1
            break
    return value


def match_complete(command):
    match command:
        case "go":
            value = 1
        case _:
            value = 2
    return value


def try_in_branch(flag):
    if flag:
        try:
            value = 1
        finally:
            pass
    return value


def with_in_branch(flag):
    if flag:
        with open("f"):
            value = 1
    return value


def assert_branch(flag):
    if flag:
        value = 1
    else:
        assert False
    return value


def continue_branch(items):
    for item in items:
        if item:
            value = 1
        else:
            continue
        print(value)


def named_else(flag):
    if flag:
        value = 1
    elif flag is None:
        value = 2
    else:
        pass
    return value


class Holder:
    if FLAG:
        attribute = 1

    method = attribute


if FLAG:
    module_value = 1

print(module_value)
print(module_value)


def parameter(flag, value=0):
    if flag:
        value = 1
    return value


def deleted(flag):
    if flag:
        value = 1
    del value


def used_twice(flag):
    if flag:
        value = 1
    return value, value


def no_scope_leak():
    if FLAG:
        input = 1
    return input


def isinstance_tests(db_field, request):
    if isinstance(db_field, (int, str)):
        if isinstance(db_field, int):
            formfield = 1
        elif isinstance(db_field, str):
            formfield = 2

        print(formfield)
    return None


def same_builtin_tests(value, other):
    if isinstance(value, int):
        number = 1
    if isinstance(other, str):
        print(number)
    return None


def different_call_tests(value, other):
    if callable(value):
        number = 1
    if callable(other):
        print(number)
    return None


def len_tests(value, other):
    if len(value):
        number = 1
    if len(other):
        print(number)
    return None


def continue_is_not_permissive(ops, flag):
    for op in ops:
        if flag:
            if not op:
                continue
            before = len(ops)
        print(op)
        if flag and before == len(ops):
            print("x")


def break_is_permissive(ops, flag):
    for op in ops:
        if flag:
            if not op:
                break
            before = len(ops)
        print(before)


def handler_name(flag):
    try:
        pass
    except ValueError as error:
        print(error)
    if flag:
        error = 1
    return error


class Cache:
    def __init__(self):
        self.store = {}

    def assigned(self, key):
        if not hasattr(self, "store"):
            store = {}
        return store[key]

    def missing(self, key):
        if not hasattr(self, "absent"):
            absent = {}
        return absent[key]
