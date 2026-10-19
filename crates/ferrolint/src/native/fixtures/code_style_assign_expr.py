import re

a = 1
if a:
    pass

b = 2
if not b:
    pass

c = 3
if c == 3:
    pass

d = 4
if d == 4 and d:
    pass

e = 5
if e > 1 < 7:
    pass

f = 6

if f:
    pass

g = 7; h = 8
if h:
    pass

i, j = 1, 2
if i:
    pass

k = l = 3
if k:
    pass

m: int = 9
if m:
    pass

n: int
if n:
    pass

o = (
    1
)
if o:
    pass

p = 10
if p == 1:
    pass
elif p == 2:
    pass

q = 11
if q == 1:
    pass
else:
    if q == 2:
        pass

r = 12
if r == 1:
    pass
else:
    pass
if r == 3:
    pass

s = 13
if s:
    pass
elif s == 2:
    pass

t = 14
if t == 1:
    pass
if t:
    pass

u = 15
if u == 1:
    pass
if u == 2 and u:
    pass

v = 16
if v == 1:
    pass
if not v:
    pass

w = re.match("x", "y")
if w:
    pass

x = re.match("x", "y")
if not x:
    pass

y = [1, 2, 3]
if y is None:
    pass

z = 1
if z.real:
    pass

aa = 1
if aa in (1, 2):
    pass

ab = 1
if -ab:
    pass

ac = "string"
if ac == "string":
    pass

ad = lambda: 1
if ad:
    pass

o_ = 1
if not o_:
    pass

no = 3
if not no:
    pass

an = 1
if an:
    pass


def function(value):
    local = value + 1
    if local:
        return 1
    other = {"a": 1}
    if other:
        return 2
    attr = value
    if attr.x:
        return 3
    if value:
        count = len(value)
        if count > 3:
            return 4
    else:
        number = 5
        if number:
            return 5
    while value:
        flag = value.pop()
        if flag:
            return 6
    return 0


class Klass:
    counter = 0
    if counter:
        pass

    def method(self):
        long_name_for_a_value = "a quite long string that makes the suggestion long enough"
        if long_name_for_a_value:
            pass
        shorter = "a quite long string that makes the suggestion long enough to wrap!!!!!"
        if shorter:
            pass


if True:
    inner = 1
    if inner:
        pass

try:
    attempt = 1
    if attempt:
        pass
except ValueError:
    handled = 2
    if handled:
        pass
finally:
    done = 3
    if done:
        pass

with open("f") as handle:
    contents = handle.read()
    if contents:
        pass

match a:
    case 1:
        case_value = 1
        if case_value:
            pass

ae = yield_value = 5
if ae:
    pass

lst = list(range(3))
if lst != []:
    pass

dct = {"k": [1, 2, 3], 'q': (1,)}
if dct:
    pass

nested = ((1 + 2) * 3) ** 2
if nested:
    pass

neg = -1
if neg < 0:
    pass

boolean = not a
if boolean:
    pass

cond = 1 if a else 2
if cond:
    pass

num = 10_000
if num:
    pass

fl = 1.5e300
if fl:
    pass

byt = b"x"
if byt:
    pass

s2 = 'it\'s'
if s2:
    pass

t2 = a, b
if t2:
    pass

l2 = [a, b]
if l2:
    pass

st = {a}
if st:
    pass

sl = lst[1:2]
if sl:
    pass

sw = (x for x in lst)
if sw:
    pass

lc = [x for x in lst]
if lc:
    pass

aw = a if b else c
if aw:
    pass

kw = dict(a=1, **dct)
if kw:
    pass

uc = lambda x, *y, z=1, **w: x
if uc:
    pass
