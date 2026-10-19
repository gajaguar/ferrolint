import contextlib


def f():
    try:
        d = {}
        d['a'] = 1
    except Exception:
        e = {}
        e['a'] = 1
    else:
        g = {}
        g['a'] = 1
    finally:
        h = {}
        h['a'] = 1
    with contextlib.suppress(Exception):
        i = {}
        i['a'] = 1
    for x in y:
        j = {}
        j['a'] = 1
    else:
        k = {}
        k['a'] = 1
    match x:
        case 1:
            l = {}
            l['a'] = 1
    if x:
        m = {}
        m['a'] = 1
    elif y:
        n = {}
        n['a'] = 1
    else:
        o = {}
        o['a'] = 1
    while x:
        while True:
            if z:
                break
            if q:
                continue


class C:
    d = {}
    d['a'] = 1

    def m(self):
        while self.x:
            if self.x:
                if self.y:
                    pass
            elif self.z:
                pass
            else:
                pass
        for x in y:
            if x:
                if y:
                    pass
            elif z:
                pass
        try:
            if x:
                if y:
                    pass
            elif z:
                pass
        except Exception:
            if x:
                if y:
                    pass
            elif z:
                pass


while True:
    if a:
        break
    elif b:
        pass
    else:
        break
while True:
    if a:
        pass
    if b:
        break
while True:
    if a:
        break
    if b:
        pass
    if c:
        break
while True:
    if a:
        break
    else:
        pass
    if b:
        break
while True:
    if a:
        break
    elif b:
        break
    else:
        pass
    if c:
        break
while True:
    if a:
        break
    else:
        if b:
            break
        else:
            pass
    if c:
        break
def gen():
    while True:
        if (yield):
            break
        if not (yield 1):
            break
        if -(yield 2):
            break
    d = {}
    d['a'] = yield 1
    e = {}
    e['a'] = (yield from x)
    while (yield):
        pass
    while (yield 3) or 2:
        if (yield 4) < 5:
            break
    h = {}
    h[(yield)] = 1
    h[3] = (yield) + 1
