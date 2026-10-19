import itertools

items = [1, 2, 3]

for a in [1, 2, 3]:
    pass

for a in (1, 2, 3):
    pass

for a in items:
    pass

for a in [*items, 4]:
    pass

for a in []:
    pass

for a in [
    1,
    2,
]:
    pass
else:
    pass

for a in [1, 2][0:1]:
    pass

for a in [[1], [2]]:
    for b in [3, 4]:
        pass

first = [x for x in [1, 2, 3]]
second = {x for x in [1, 2, 3]}
third = {x: x for x in [1, 2, 3]}
fourth = (x for x in [1, 2, 3])
fifth = [x for x in (1, 2, 3)]
sixth = [x for x in [1, 2] for y in [3, 4] if y in [5]]
seventh = [x for x in [1, 2] if x in [1, 2]]
eighth = [x for x in itertools.chain([1], [2])]
ninth = [x for x in [i for i in [1, 2]]]


def function():
    for a in [1, 2, 3]:
        yield a
    return [a for a in [1]]


async def coroutine(aiter):
    for a in [1, 2, 3]:
        pass
    async for a in [1, 2, 3]:
        pass
    async for a in aiter:
        pass
    return [a async for a in [1, 2]]


class Container:
    for a in [1, 2]:
        pass
    values = [a for a in [1, 2]]
