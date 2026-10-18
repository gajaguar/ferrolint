import builtins
import socket
from builtins import Exception as Exc


class Plain:
    pass


class Child(Plain):
    pass


class Good(ValueError):
    pass


class Mixed(Plain, ValueError):
    pass


class Meta(type):
    pass


Alias = Exception
Alias2 = Alias


def f(x, flag):
    try:
        pass
    except Exception:
        pass
    try:
        pass
    except BaseException as err:
        print(err)
    try:
        pass
    except (ValueError, Exception):
        pass
    try:
        pass
    except (Exception, BaseException):
        pass
    try:
        pass
    except [Exception]:
        pass
    try:
        pass
    except Exception:
        raise
    try:
        pass
    except Exception:
        if flag:
            raise
    try:
        pass
    except Exception:
        x = 1
        raise ValueError("a")
    try:
        pass
    except Exception as e:
        raise ValueError("a") from e
    try:
        pass
    except builtins.Exception:
        pass
    try:
        pass
    except Exc:
        pass
    try:
        pass
    except Alias:
        pass
    try:
        pass
    except Alias2:
        pass
    try:
        pass
    except ValueError:
        pass
    try:
        pass
    except (ValueError):
        pass
    try:
        pass
    except socket.error:
        pass
    try:
        pass
    except* Exception:
        pass
    try:
        pass
    except* (OSError, Exception):
        pass
    try:
        pass
    except Plain:
        pass
    try:
        pass
    except (Plain, Child):
        pass
    try:
        pass
    except Good:
        pass
    try:
        pass
    except Mixed:
        pass
    try:
        pass
    except Meta:
        pass
    try:
        pass
    except None:
        pass
    try:
        pass
    except 5:
        pass
    try:
        pass
    except (Plain, x):
        pass
    try:
        pass
    except Exception or ValueError:
        pass
    try:
        pass
    except (ValueError | OSError):
        pass
    try:
        pass
    except (ValueError, OSError) + (TypeError,):
        pass
    try:
        pass
    except ValueError < OSError:
        pass
    try:
        pass
    except ValueError + OSError:
        pass
    try:
        pass
    except Plain | Child:
        pass


def g(x):
    raise Exception
    raise Exception("a")
    raise BaseException("a")
    raise ValueError("a")
    raise Alias("a")
    raise Alias2
    raise builtins.Exception("a")
    raise Exc("a")
    raise Exception("a") from x
    raise x
    raise x("a")
    raise Plain
    raise Plain()
    raise Child("a")
    raise Good
    raise Mixed()
    raise Meta
    raise "string"
    raise 5
    raise 1.5
    raise None
    raise True
    raise b"bytes"
    raise f"{x}"
    raise (ValueError, OSError)
    raise [ValueError]
    raise {"a": 1}
    raise {1, 2}
    raise ...
    raise NotImplemented
    raise ValueError from 5
    raise ValueError from "s"
    raise ValueError from None
    raise ValueError from Plain
    raise ValueError from Plain()
    raise ValueError from x
    raise ValueError("a") from ValueError
    raise f
    raise lambda: 1
    raise x + 1
    raise -1
    raise not x
    raise x.y
    raise x.y("a")
