import abc


class Base:
    def __init__(self):
        self.base = 1


class Middle(Base):
    def __init__(self):
        super().__init__()
        self.middle = 1


class Other:
    def __init__(self):
        self.other = 1


class Empty:
    def __init__(self):
        pass


class Leaf(Middle):
    def __init__(self):
        Base.__init__(self)
        Middle.__init__(self)


class Leaf2(Middle, Other):
    def __init__(self):
        Middle.__init__(self)
        Other.__init__(self)
        Base.__init__(self)


class Leaf3(Middle):
    def __init__(self):
        Other.__init__(self)
        super().__init__()
        Base.__init__(self)


class Leaf4(Middle):
    def __init__(self):
        Base.__init__(self)
        Middle.__init__(self)
        Middle.__init__(self)


class Leaf5(Empty):
    def __init__(self):
        Empty.__init__(self)
        Empty.__init__(self)
        Base.__init__(self)


class Leaf6:
    def __init__(self):
        Base.__init__(self)
        object.__init__(self)


class Leaf7(Base):
    def __init__(self):
        object.__init__(self)
        Exception.__init__(self)


class Leaf8(Base, Exception):
    def __init__(self):
        Base.__init__(self)
        Exception.__init__(self)
        self.__init__()


class Leaf9(Base):
    def __init__(self):
        def inner():
            Other.__init__(self)

        inner()
        Unknown.__init__(self)
        self.helper.__init__()
        Other().__init__()


class Leaf10(Base):
    def reset(self):
        Other.__init__(self)

    def __init__(self, *args):
        Other.__init__(self, *args)
        Leaf10.__init__(self)


class Leaf11(Base, abc.ABC):
    def __init__(self):
        Base.__init__(self)
        abc.ABC.__init__(self)
        Other.__init__(self)
