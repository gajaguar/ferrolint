import os
import sys
import sysconfig


def find_ferrolint_bin() -> str:
    """Return the path of the ferrolint executable the wheel installed."""
    exe = "ferrolint" + sysconfig.get_config_var("EXE")
    scripts = sysconfig.get_path("scripts")
    candidates = [os.path.join(scripts, exe)]
    if sys.version_info >= (3, 10):
        user_scheme = sysconfig.get_preferred_scheme("user")
        candidates.append(os.path.join(sysconfig.get_path("scripts", scheme=user_scheme), exe))
    for path in candidates:
        if os.path.isfile(path):
            return path
    raise FileNotFoundError(candidates[0])
