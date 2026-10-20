import socket
import builtins
import select
import os
from os import error as os_error


class MyErr(Exception):
    pass


class Sub(MyErr):
    pass


class Other(ValueError, KeyError):
    pass


def f():
    try:
        pass
    except (ValueError, ValueError):  # same
        pass
    except (Exception, ValueError):  # anc
        pass
    except (ValueError, Exception):  # anc reversed
        pass
    except (IOError, OSError):  # same alias
        pass
    except (EnvironmentError, OSError, IOError):
        pass
    except (socket.error, OSError):  # socket.error
        pass
    except (select.error, OSError):
        pass
    except (os.error, OSError):
        pass
    except (os_error, OSError):
        pass
    except (MyErr, Sub):
        pass
    except (Sub, MyErr, Exception):
        pass
    except (Other, ValueError, KeyError, LookupError):
        pass
    except [ValueError, Exception]:
        pass
    except (builtins.ValueError, ValueError):
        pass
    except (FileNotFoundError, PermissionError, OSError):
        pass
    except (KeyError, IndexError, LookupError):
        pass
    except (ValueError, TypeError):
        pass
    except ValueError:
        pass
    except (ValueError, (TypeError, Exception)):
        pass
    except (ValueError, *[Exception]):
        pass
    except (a.b, ValueError):
        pass
    except ExceptionGroup:
        pass
    except (BaseExceptionGroup, ExceptionGroup, Exception, BaseException):
        pass
    except (ValueError or TypeError):
        pass
    except (Exception, ValueError, TypeError, ValueError):
        pass
    except (UnicodeError, UnicodeDecodeError):
        pass
    except (ModuleNotFoundError, ImportError):
        pass
    except (Warning, UserWarning, ResourceWarning):
        pass
    except (StopIteration, StopAsyncIteration, GeneratorExit):
        pass
    except (socket.timeout, TimeoutError):
        pass
    except (socket.gaierror, OSError):
        pass


try:
    pass
except (ValueError, ValueError):
    pass


class C:
    try:
        pass
    except (ValueError, ValueError):
        pass


def star():
    try:
        pass
    except* (ValueError, ValueError):
        pass


def unknown_base():
    from somewhere import Base

    class Mine(Base):
        pass

    try:
        pass
    except (Mine, Exception):
        pass
    except (Base, Exception):
        pass
    except (Mine, Base):
        pass
