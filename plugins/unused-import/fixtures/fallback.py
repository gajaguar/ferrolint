"""An import with a fallback, and blocks the fix must keep valid."""
try:
    import tomllib
except ImportError:
    tomllib = None

if True:
    import zlib

for _ in range(1):
    import gzip
