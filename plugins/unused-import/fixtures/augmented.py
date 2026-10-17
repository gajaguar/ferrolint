"""An augmented assignment reads the name it rebinds."""

from settings import APPS
from settings import MIDDLEWARE

APPS += ("silk",)
MIDDLEWARE = MIDDLEWARE + ("silk",)
