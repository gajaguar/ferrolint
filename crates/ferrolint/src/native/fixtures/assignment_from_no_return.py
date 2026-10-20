import functools


def decorate(func):
    return func


def no_return():
    pass


def only_none():
    return None


def bare_return():
    return


def mixed(flag):
    if flag:
        return 1
    return None


def value_and_bare(flag):
    if flag:
        return
    return 1


def docstring_only():
    """Nothing else."""


def ellipsis_only():
    ...


def raises():
    raise ValueError


def raises_with_docstring():
    """Raises."""
    raise ValueError


def ends_with_raise(flag):
    if flag:
        print(flag)
    raise ValueError


def raises_after_return(flag):
    if flag:
        return None
    raise ValueError


def abstract():
    raise NotImplementedError


def abstract_documented():
    """Subclasses fill it in."""
    raise NotImplementedError("later")


def not_first_statement():
    print("x")
    raise NotImplementedError


def generator():
    yield


def generator_from():
    yield from []


async def coroutine():
    pass


@decorate
def decorated():
    pass


@functools.lru_cache
def cached():
    pass


def nested_return():
    def inner():
        return 1

    inner()


def nested_none():
    def inner():
        return 1

    return None


def loop_only(items):
    for item in items:
        print(item)


def sort():
    pass


def reverse():
    return None


def defined_twice():
    pass


def defined_twice():
    return 1


def with_lambda():
    return lambda: 1


def returns_value():
    return 1


def returns_name(value):
    return value


def returns_none_name():
    none = None
    return none


def in_module():
    first = no_return()
    second = only_none()
    third = bare_return()
    fourth = mixed(1)
    fifth = value_and_bare(1)
    sixth = docstring_only()
    seventh = ellipsis_only()
    eighth = raises()
    ninth = raises_with_docstring()
    tenth = ends_with_raise(1)
    eleventh = raises_after_return(1)
    twelfth = abstract()
    thirteenth = abstract_documented()
    fourteenth = not_first_statement()
    fifteenth = generator()
    sixteenth = generator_from()
    seventeenth = coroutine()
    eighteenth = decorated()
    nineteenth = cached()
    twentieth = nested_return()
    twenty_first = nested_none()
    twenty_second = loop_only([])
    twenty_third = sort()
    twenty_fourth = reverse()
    twenty_fifth = defined_twice()
    twenty_sixth = with_lambda()
    twenty_seventh = returns_value()
    twenty_eighth = returns_name(1)
    twenty_ninth = returns_none_name()
    return (
        first,
        second,
        third,
        fourth,
        fifth,
        sixth,
        seventh,
        eighth,
        ninth,
        tenth,
        eleventh,
        twelfth,
        thirteenth,
        fourteenth,
        fifteenth,
        sixteenth,
        seventeenth,
        eighteenth,
        nineteenth,
        twentieth,
        twenty_first,
        twenty_second,
        twenty_third,
        twenty_fourth,
        twenty_fifth,
        twenty_sixth,
        twenty_seventh,
        twenty_eighth,
        twenty_ninth,
    )


def forms():
    a, b = no_return()
    c = d = only_none()
    e: int = no_return()
    f = [no_return()]
    g = (no_return())
    h = no_return() or 1
    i = (
        no_return()
    )
    j = print()
    k = len([])
    l = print("a") if a else None
    m = undefined_function()
    e += no_return()
    return a, b, c, d, e, f, g, h, i, j, k, l, m


def local_function():
    def nothing():
        pass

    def nothing_again():
        return None

    result = nothing()
    other = nothing_again()
    return result, other


def shadowed(no_return):
    return no_return()


class Owner:
    def nothing(self):
        pass

    def none(self):
        return None

    def value(self):
        return 1

    def sort(self):
        pass

    def reverse(self):
        pass

    @staticmethod
    def static_nothing():
        pass

    @classmethod
    def class_nothing(cls):
        pass

    @property
    def prop(self):
        pass

    def abstract(self):
        raise NotImplementedError

    def uses(self):
        a = self.nothing()
        b = self.none()
        c = self.value()
        d = self.sort()
        e = self.reverse()
        f = self.static_nothing()
        g = self.class_nothing()
        h = self.prop
        i = self.abstract()
        j = self.missing()
        k = no_return()
        return a, b, c, d, e, f, g, h, i, j, k

    def replaced(self):
        self.nothing = lambda: 1
        return self.nothing()

    def other(self, owner):
        a = owner.nothing()
        return a

    def nested(self):
        def inner(self):
            return self.nothing()

        a = inner(self)
        return a


class Child(Owner):
    def uses_parent(self):
        return self.nothing()

    def assigns_parent(self):
        a = self.nothing()
        return a


def method_of_a_class_body():
    class Local:
        def nothing(self):
            pass

        def uses(self):
            return self.nothing()

        def assigns(self):
            a = self.nothing()
            return a

    return Local
