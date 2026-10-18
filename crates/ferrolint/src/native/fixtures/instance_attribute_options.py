class Configured:
    def setup(self):
        self.a = 1

    def other(self):
        self.b = 1

    def __init__(self):
        self.c = 1


class ThingHelper:
    def run(self):
        print(self.d)
        self.d = 1


class ThingMixin:
    def run(self):
        print(self.e)
        self.e = 1
