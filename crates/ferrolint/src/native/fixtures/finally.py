def f():
    try:
        pass
    finally:
        return 1


for _ in []:
    try:
        pass
    finally:
        break

try:
    pass
finally:
    for _ in []:
        break
