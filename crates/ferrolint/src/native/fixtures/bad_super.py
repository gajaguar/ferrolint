class Root:
    def run(self):
        pass


class Base(Root):
    def run(self):
        pass


class Other:
    pass


class Child(Base):
    def own(self):
        super(Child, self).run()

    def ancestor(self):
        super(Root, self).run()

    def zero_arguments(self):
        super().run()

    def by_type(self):
        super(type(self), self).run()

    def by_class(self):
        super(self.__class__, self).run()

    def unrelated(self):
        super(Other, self).run()

    def nested_function(self):
        def inner():
            super(Other, self).run()

        return inner

    def nested_class(self):
        class Nested(Other):
            def run(self):
                super(Child, self).run()

        return Nested

    def not_a_method_call(self):
        return super(Other, self)
