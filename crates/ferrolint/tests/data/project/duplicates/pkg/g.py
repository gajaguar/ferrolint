"""Module g: the same block, disabled for the function."""


class Other:
    def method(self):
        # pylint: disable=duplicate-code
        one = 1
        two = 2
        three = 3
        four = 4
        five = 5
        return five
