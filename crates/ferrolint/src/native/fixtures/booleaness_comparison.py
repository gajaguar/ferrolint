"""C1803: a comparison with an empty list, tuple or dict."""

lst = [1]
tup = (1,)
dct = {1: 2}
st = {1}
txt = "a"
byt = b"a"
num = 1
flt = 1.5
flag = True
listcall = list()
dictcall = dict()
strcall = str()
tuplecall = tuple()
twice = []
twice = 1
alias = lst

if lst == []:
    pass
if tup == ():
    pass
if dct == {}:
    pass
if st == []:
    pass
if txt == []:
    pass
if byt == []:
    pass
if num == []:
    pass
if flt == []:
    pass
if flag == []:
    pass
if listcall == []:
    pass
if dictcall == {}:
    pass
if strcall == []:
    pass
if tuplecall == ():
    pass
if twice == []:
    pass
if alias == []:
    pass
if [] == lst:
    pass
if [] == [1]:
    pass
if [] == []:
    pass
if [1] != ():
    pass
if (1, 2) == []:
    pass
if "x" != []:
    pass
if lst != []:
    pass
if lst < []:
    pass
if lst >= {}:
    pass
if lst is []:
    pass
if lst is not []:
    pass
if lst in []:
    pass
if lst == [] == lst:
    pass
if list() == []:
    pass
if (lst) == []:
    pass
if lst == [] and txt:
    pass

x = lst == []
y = lst != []
z = not lst != []
w = bool(lst != [])
v = [a for a in lst if a != []]
u = any(a != [] for a in lst)
t = 1 if lst != [] else 2
s = lst != [] and lst
r = lst != [] or txt
q = (lst != []) + 1
p = print(lst != [])
if lst != [] or txt:
    pass
if txt and (lst != [] and txt):
    pass
f = lambda q: q != []
print(list(filter(lambda q: q != [], lst)))
print(list(filter(None, [lst != []])))
assert lst != []
assert txt, lst != []
while lst != []:
    break
lst != []


def local():
    items = []
    names = ()
    return items == [], names != ()


class Holder:
    members = {}
    check = members == {}

    def method(self):
        return lst == []


def rebound(flag):
    same = []
    same = [1]
    mixed = []
    if flag:
        mixed = ()
    loop = []
    for loop in flag:
        pass
    return same == [], mixed == [], loop == []
