class Plain:
    def build(cls):
        return cls

    def helper():
        return 1

    build = classmethod(build)
    helper = staticmethod(helper)

    def other(cls):
        return cls

    alias = classmethod(other)
    unrelated = classmethod(missing)
    not_a_method = 1
    wrapped = classmethod(not_a_method)
    called = classmethod(other())
    attribute = classmethod(Plain.other)
    keyword = classmethod(f=other)
    both = classmethod(other), 1
    chained = copy = staticmethod(other)


class Later:
    value = 1

    def value(self):
        return 1

    value = classmethod(value)


class Earlier:
    def spam(self):
        return 1

    spam = 1
    other = staticmethod(spam)


class Branches:
    if True:

        def first(cls):
            return cls

        first = classmethod(first)
    else:

        def second():
            return 2

        second = staticmethod(second)

    try:
        pass
    except ValueError:
        third = classmethod(first)


class Nested:
    def method(cls):
        return cls

    def inner(self):
        method = classmethod(self.method)
        other = classmethod(Nested.method)
        return method, other

    class Inside:
        method = classmethod(method)

    lambda_ = lambda: staticmethod(method)


def module_level():
    pass


module_level = staticmethod(module_level)
x = classmethod(module_level)


def function_scope():
    def local(cls):
        return cls

    local = classmethod(local)
    return local


class Starred:
    def spam(cls):
        return cls

    star = classmethod(*[spam])
    rename = staticmethod(spam, 1)
    aliased = my_classmethod(spam)
    annotated: object = classmethod(spam)
    ann_only: object
