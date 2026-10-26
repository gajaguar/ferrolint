items = [1, 2, 3]
empty = []


def after_loop(values):
    for value in values:
        pass
    print(value)
    return value


def range_loop():
    for i in range(3):
        pass
    return i


def enumerate_loops(values):
    for index, value in enumerate(range(3)):
        pass
    for other, thing in enumerate(values):
        pass
    return index, value, other, thing


def list_loops():
    for a in [1, 2]:
        pass
    for b in []:
        pass
    for c in (1, 2):
        pass
    for d in ():
        pass
    for e in {1: 2}:
        pass
    for f in {}:
        pass
    for g in items:
        pass
    for h in empty:
        pass
    return a, b, c, d, e, f, g, h


def else_loops(values):
    for a in values:
        pass
    else:
        return None
    for b in values:
        pass
    else:
        raise ValueError
    for c in values:
        pass
    else:
        print("done")
    return a, b, c


def reassigned(values):
    for a in values:
        pass
    a = 1
    for b in values:
        pass
    for b in values:
        pass
    c = 0
    for c in values:
        pass
    return a, b, c


def inside_loop(values):
    for a in values:
        print(a)
    else:
        print(a)
    for b in values:
        def inner():
            return b

        inner()
    return inner


def nested_use(values):
    for a in values:
        pass

    def later():
        return a

    return later, (lambda: a), [a for _ in values]


for module_loop in items:
    pass
print(module_loop)


def uses_module_loop():
    return module_loop


def conditional(values, flag):
    if flag:
        for a in values:
            pass
    return a


def tuple_target(pairs):
    for k, v in pairs:
        pass
    return k, v


def augmented(values):
    for total in values:
        pass
    total += 1
    del total


class Holder:
    for attr in items:
        pass
    print(attr)


def nested_binding_in_loop(names):
    for pos, name in enumerate(names):
        if name:
            pos -= 1
        break
    return names[pos + 1:]


def exclusive_use(pairs):
    try:
        for pk in pairs:
            pass
    except Exception:
        return pk
    return None


def loop_over_conditional_list(options, shells):
    available = [options] if options else shells
    for shell in available:
        pass
    return shell


def loop_over_alias(values):
    names = values
    for name in names:
        pass
    return name


from six.moves import range


def six_range(values):
    for i in range(len(values)):
        pass
    return i


class Holder:
    def __init__(self):
        self.full = [self]
        self.empty = []
        self.calls = make()

    def over_full(self):
        for item in self.full:
            pass
        return item

    def over_empty(self):
        for item in self.empty:
            pass
        return item

    def over_call(self):
        for item in self.calls:
            pass
        return item

    def over_other(self, other):
        for item in other.full:
            pass
        return item


import sys


def over_live_containers():
    for path in sys.path:
        pass
    for importer in enumerate(sys.meta_path):
        pass
    return path, importer
