import urllib.request
import requests
from urllib.request import urlopen

urllib.request.urlopen("http://a")
urllib.request.urlopen("http://a", timeout=1)
urlopen("http://a")
requests.get("http://a")
requests.post("http://a")
