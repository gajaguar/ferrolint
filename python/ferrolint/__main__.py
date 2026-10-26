import os
import subprocess
import sys

from ferrolint import find_ferrolint_bin

if __name__ == "__main__":
    ferrolint = find_ferrolint_bin()
    if sys.platform == "win32":
        # os.execvp does not replace the process on Windows.
        try:
            sys.exit(subprocess.run([ferrolint, *sys.argv[1:]]).returncode)
        except KeyboardInterrupt:
            sys.exit(2)
    os.execvp(ferrolint, [ferrolint, *sys.argv[1:]])
