"""auth.User in a docstring."""
import json

import django.http
from django import forms
from django.contrib.auth.models import Group, User
from django.contrib.auth.models import *
from django.contrib.auth.models import Permission
from django.db import models
from django.db.models import Model
from django.http import HttpResponse, JsonResponse
from django.utils.html import format_html, format_html_join, mark_safe

OWNER = "auth.User"
OTHER = "auth.Group"
JOINED = "auth." "User"
BYTES = b"auth.User"


class Plain(models.Model):
    __unicode__ = 5

    def method(self):
        """auth.User"""
        return "auth.User"


class Callable(models.Model):
    __unicode__ = lambda self: "x"


class Aliased(Model):
    __unicode__ = None


class Fine(models.Model):
    __unicode__ = str


class Annotated(models.Model):
    __unicode__: int = 5


class Tuple(models.Model):
    __unicode__ = ("a", "b")


class Has(models.Model):
    def __unicode__(self):
        return "x"


class HasAfterAbstract(models.Model):
    class Meta:
        abstract = True

    def __unicode__(self):
        return "x"


class HasBeforeAbstract(models.Model):
    def __unicode__(self):
        return "x"

    class Meta:
        abstract = True


class NotAbstract(models.Model):
    class Meta:
        abstract = False

    def __unicode__(self):
        return "x"


class Child(Has):
    pass


class ChildStr(Has):
    def __str__(self):
        return "x"


class Decorated:
    def __str__(self):
        return "x"


def python_2_unicode_compatible(cls):
    return cls


@python_2_unicode_compatible
class Compat(models.Model):
    def __str__(self):
        return "x"


class FromCompat(Compat):
    pass


class FromCompatStr(Compat):
    def __str__(self):
        return "y"


class Mixin:
    def __unicode__(self):
        return "x"


class MixedIn(Mixin, models.Model):
    pass


class NotModel:
    def __unicode__(self):
        return "x"


class MetaForm(forms.ModelForm):
    class Meta:
        model = User
        exclude = ["a"]


class Other(forms.ModelForm):
    class Meta:
        model = User
        fields = ["a"]


class Annot(forms.ModelForm):
    class Meta:
        exclude: list = []


class Chain(forms.ModelForm):
    class Meta:
        a = exclude = 1


class Plainform(forms.Form):
    class Meta:
        exclude = 1


class SubForm(MetaForm):
    class Meta:
        exclude = 1


class NoMeta(forms.ModelForm):
    exclude = 1


def responses(data, name):
    HttpResponse(json.dumps(data))
    HttpResponse(json.dumps(data), content_type="application/json")
    HttpResponse(data, content_type="text/html")
    django.http.HttpResponse(json.dumps(data))
    HttpResponse(data=json.dumps(data))
    HttpResponse(*[json.dumps(data)])
    JsonResponse(data, content_type="application/json")
    JsonResponse(data)
    django.http.JsonResponse(data, content_type=None)
    HttpResponse(data, content_type=("Application/JSON"))
    HttpResponse(data, content_type=f"application/{name}")
    HttpResponse(data, content_type=f"application/json{name}")
    HttpResponse(data, content_type="x", **{})


def html(name, rows):
    format_html(f"<b>{name}</b>")
    format_html(f"<b>x</b>")
    format_html("<b>{}</b>".format(name))
    format_html("<b>%s</b>" % name)
    format_html("<b>" + name + "</b>")
    format_html("<b>" + name)
    format_html(name + "<b>")
    format_html("a" + "b")
    format_html("a" + "b" + name)
    format_html("<b>{}</b>", name)
    format_html(name)
    format_html()
    format_html(*rows)
    format_html(format_string=f"<b>{name}</b>")
    format_html_join(f"{name}", "{}", rows)
    format_html_join("", f"{name}", rows)
    format_html_join("", "{}", rows)
    format_html_join(f"{name}")
    format_html_join(sep=f"{name}")
    mark_safe(f"<b>{name}</b>")
    mark_safe("<b>{}</b>".format(name))
    mark_safe("<b>%s</b>" % name)
    mark_safe("<b>" + name)
    mark_safe("<b>x</b>")
    mark_safe(name)
    mark_safe(("<b>" + name))
    mark_safe(name.format(1))
    mark_safe(str.format("x"))
    mark_safe(b"<b>" + name)
    mark_safe(1 + name)
    mark_safe(name % name)
    mark_safe("a" "b" + name)
    mark_safe("a" % ())
    utils.html.mark_safe(f"{name}")
    my_mark_safe(f"{name}")
    mark_safe(*[f"{name}"])
