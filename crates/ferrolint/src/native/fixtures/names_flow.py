import os
try:
    import json
except ImportError:
    json = None
try:
    import simplejsonX as sjson
except ImportError:
    sjson = None
try:
    import yaml
except ImportError:
    yaml = None
try:
    from fooLib import barLib
except ImportError:
    barLib = 5
try:
    from foo import X as xX
except ImportError:
    xX = 5
if os.name == "nt":
    plat = 1
else:
    plat = 2
if os.name == "nt":
    platB = 1
elif os.sep:
    platB = 2
else:
    platB = 3
if os.name == "nt":
    platC = 1
platC = 2
reassigned = 1
reassigned = 2
reassignedLater = 1
print(reassignedLater)
reassignedLater = 2
AUPPER = 1
AUPPER = 2
for loopVar in range(3):
    inLoop = 1
    inLoopB = 2
while True:
    inWhile = 1
    break
with open("f") as withVar:
    withBody = 1
if True:
    ifBody = 1
def f():
    global glob, GLOB
    glob = 1
    GLOB = 2
    camelLocal = 1
    CONST_LOCAL = 2
    a, b = 1, 2
    for loopVar2 in range(3):
        pass
    with open("x") as withLocal:
        pass
    try:
        pass
    except Exception as exCaught:
        pass
    if (walrusVar := 5):
        pass
    [compVar for compVar in range(3)]
    lam = lambda lamArg: lamArg
    camelLocal += 1
    def nested(): pass
    def nestedCamel(): pass
    class NestedClass: pass
    class nestedClass: pass
    Alias = NestedClass
    alias = NestedClass
    Alias2 = os
    for Model in (NestedClass,):
        pass
match 5:
    case matchCap:
        pass
if __name__ == "__main__":
    mainVar = 1
    MAIN_CONST = 2
    main_lower = 3
    mainCamel = os
    MainCamel = os
    mainReassigned = 1
    mainReassigned = 2
