"""Import in a try block."""
try:
    import top.t2 as other
except ImportError:
    other = None
