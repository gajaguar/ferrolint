import socket


class MyErr(ValueError):
    pass


class Sub(MyErr):
    pass


def a():
    try:
        pass
    except Exception:
        pass
    except ValueError:
        pass


def b():
    try:
        pass
    except (OSError, KeyError):
        pass
    except FileNotFoundError:
        pass
    except IOError:
        pass
    except LookupError:
        pass
    except IndexError:
        pass


def c():
    try:
        pass
    except MyErr:
        pass
    except Sub:
        pass
    except ValueError:
        pass


def d():
    try:
        pass
    except ValueError:
        pass
    except MyErr:
        pass
    except (Sub, TypeError):
        pass


def e():
    try:
        pass
    except BaseException:
        pass
    except (Exception, KeyboardInterrupt):
        pass


def f(x):
    try:
        raise ValueError("x")
    except ValueError:
        return 1
    return 2


def g(x):
    try:
        if x:
            return 1
        raise ValueError("x")
    except ValueError:
        pass


def h(x):
    try:
        if x:
            return 1
        raise ValueError("x")
    except TypeError:
        pass


def i(x):
    try:
        if x:
            return 1
        raise SomeThing()
    except TypeError:
        pass


def j(x):
    if x:
        return 1
    for y in x:
        if y:
            return 2
    else:
        return 3


def k(x):
    with x:
        return 1


def l(x):
    for y in x:
        return 1


def m(x):
    while x:
        return 1


def n(x):
    while 1:
        x += 1
    return 3


def o(x):
    match x:
        case 1:
            return 1
        case _:
            return 2


def p(x):
    def inner():
        return 1
    if x:
        return 2


def q(x):
    if x:
        return 1
    def inner():
        return 1


def r(x):
    if x:
        return 1
    x = sys.exit()
