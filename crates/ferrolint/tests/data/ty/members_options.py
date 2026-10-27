"""One class per option Pylint reads for `no-member`."""


class Skipped:
    def reads(self):
        return self.absent


class FieldMixin:
    def reads(self):
        return self.absent


class FieldHelper:
    def reads(self):
        return self.absent


class Plain:
    name = "x"
    names = "y"

    def reads(self):
        return self.ghost_one, self.nme, self.nam
