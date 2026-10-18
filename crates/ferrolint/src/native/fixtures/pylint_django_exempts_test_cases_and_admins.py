import unittest

from django.contrib import admin
from django.test import SimpleTestCase
from django.test import TestCase
from django.test import TransactionTestCase
from rest_framework.test import APITestCase


class ATest(TestCase):
    def a(self):
        pass

    def b(self):
        pass

    def c(self):
        pass


class BTest(APITestCase):
    def a(self):
        pass

    def b(self):
        pass

    def c(self):
        pass


class CTest(TransactionTestCase):
    def a(self):
        pass

    def b(self):
        pass

    def c(self):
        pass


class DTest(unittest.TestCase):
    def a(self):
        pass

    def b(self):
        pass

    def c(self):
        pass


class EAdmin(admin.ModelAdmin):
    def a(self):
        pass

    def b(self):
        pass

    def c(self):
        pass


class FTest(ATest):
    def a(self):
        pass

    def b(self):
        pass

    def c(self):
        pass


class GTest(SimpleTestCase):
    def a(self):
        pass

    def b(self):
        pass

    def c(self):
        pass


class HTest(object):
    def a(self):
        pass

    def b(self):
        pass

    def c(self):
        pass


class Plain(TestCase):
    def a(self):
        pass

    def b(self):
        pass

    def c(self):
        pass


class Y(admin.ModelAdmin):
    def a(self):
        pass

    def b(self):
        pass

    def c(self):
        pass


class IAdmin(TestCase):
    def a(self):
        pass

    def b(self):
        pass

    def c(self):
        pass


class JTest(admin.ModelAdmin):
    def a(self):
        pass

    def b(self):
        pass

    def c(self):
        pass


class Outer:
    class Inner(TestCase):
        def a(self):
            pass

        def b(self):
            pass

        def c(self):
            pass

    class InnerAdmin(admin.ModelAdmin):
        def a(self):
            pass

        def b(self):
            pass

        def c(self):
            pass
