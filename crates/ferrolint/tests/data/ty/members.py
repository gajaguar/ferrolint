"""`self` and `cls` members that exist, and those that do not."""
from missing_module import Unknown


class Plain:
    size = 1

    def __init__(self):
        self.name = "x"

    def reads(self):
        print(self.name, self.size)
        print(self.nme)
        print(self.absent)
        del self.gone
        self.stored = self.name

    @classmethod
    def build(cls):
        return cls.size, cls.nothing

    def guarded(self):
        try:
            return self.maybe
        except AttributeError:
            return None


class Child(Plain):
    def more(self):
        return self.name, self.lacking


class Dynamic:
    def __getattr__(self, name):
        return name

    def reads(self):
        return self.anything


class FieldMixin:
    def reads(self):
        return self.anything


class FromUnknown(Unknown):
    def reads(self):
        return self.anything


class Holder:
    def reads(self, other):
        return other.anything
