import requests
import requests as r
from requests import get, post as send
from requests.api import put
import requests.api
import urllib.request

requests.get("http://a")
requests.get("http://a", timeout=3)
requests.post("http://a", data={}, timeout=None)
requests.delete("http://a")
requests.head("http://a")
requests.options("http://a")
requests.patch("http://a")
requests.put("http://a")
requests.request("GET", "http://a")
requests.api.get("http://a")
requests.api.get("http://a", timeout=1)
r.get("http://a")
get("http://a")
send("http://a")
put("http://a", timeout=1)
requests.Session().get("http://a")
requests.get("http://a", **{"timeout": 3})
requests.get("http://a", **{"verify": False})
kwargs = {"timeout": 3}
requests.get("http://a", **kwargs)


def wrapper(**options):
    requests.get("http://a", **options)
    return requests.get("http://a")


def shadow(requests):
    requests.get("http://a")


class Client:
    def fetch(self):
        return requests.get("http://a"), urllib.request.urlopen("http://a")
