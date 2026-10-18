import os
from sys import path

global MODULE_LEVEL
count = 0


def reads():
    global count
    return count


def writes():
    global count
    count += 1


def deletes():
    global count
    del count


def undefined():
    global brand_new
    brand_new = 1


def undefined_unassigned():
    global nowhere
    return nowhere


def imported():
    global os
    return os


def imported_from():
    global path
    path = 1


def partly(first, second):
    global count, brand_new, nowhere
    count = first
    return second


def defined_later():
    global later
    return later


later = 1


def a_function():
    global helper
    helper = None


def helper():
    pass


def conditional_function():
    global conditional
    conditional = 1


if os.name == "nt":

    def conditional():
        pass


class Holder:
    global count
    value = 1

    def method(self):
        global value
        value = 2


def same_line():
    global same_line_name; same_line_name = 1


def nested():
    global from_nested

    def inner():
        global from_nested
        from_nested = 1

    return inner


def reassign_before():
    local = 1
    global local_two
    local_two = 2
    global count
    count = 3


for loop_name in range(2):
    pass


def uses_loop():
    global loop_name
    loop_name = 1
