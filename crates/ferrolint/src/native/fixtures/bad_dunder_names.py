import functools


class Good:
    def __init__(self):
        pass

    def __repr__(self):
        return ""

    def __ok__(self):
        pass

    def __fine__(self):
        pass

    def __aiter__(self):
        pass

    def __add_alias__(self):
        pass

    def _add_alias_(self):
        pass

    def _missing_(self):
        pass

    def __post_init__(self):
        pass

    def _private(self):
        pass

    def public_(self):
        pass

    def __name_mangled(self):
        pass

    def __format__(self, spec):
        return ""


class Bad:
    def __init_(self):
        pass

    def __repr(self):
        pass

    def __lenn__(self):
        pass

    def _(self):
        pass

    def __(self):
        pass

    def _spam_(self):
        pass

    @staticmethod
    def __static__():
        pass

    @classmethod
    def __cls__(cls):
        pass

    @property
    def __prop__(self):
        pass

    @functools.cache
    def __cached__(self):
        pass

    async def __async__(self):
        pass

    if True:

        def __branch__(self):
            pass

    else:

        def __other_branch__(self):
            pass

    try:

        def __in_try__(self):
            pass

    except ValueError:
        pass

    class __Inner__:
        def __inner__(self):
            pass

    def method(self):
        def __nested__():
            pass

        return __nested__


def __module_level__():
    pass


def function():
    class Local:
        def __local__(self):
            pass

    return Local
