"""Module h: the same block again, in a file that disables the rule for itself."""
# pylint: disable=duplicate-code


class Third:
    def method(self):
        one = 1
        two = 2
        three = 3
        four = 4
        five = 5
        return five
