class A:
    def a():
        pass

    def b(*args):
        pass

    def c(*, k):
        pass

    def d(**kw):
        pass

    @staticmethod
    def e():
        pass

    @classmethod
    def f():
        pass

    def __new__():
        pass

    @property
    def g():
        pass

    def i(self, /):
        pass

    if True:
        def k():
            pass

    def l(self):
        def inner():
            pass

        class Local:
            def m():
                pass


def func():
    pass
