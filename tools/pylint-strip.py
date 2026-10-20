"""Remove the "# pylint:" pragmas from the Python files given, in place.

Only real comments lose their pragma: the tokenizer finds them, so the same
text inside a string stays. Line numbers hold: a pragma on a line of its own
leaves the line empty, a trailing one is cut. A line the tokenizer does not
reach gets the plain regex instead.

shortcut: a line the tokenizer does not reach keeps the regex, so a pragma-like
string on it is still cut; upgrade when such a file matters.
"""

import io
import re
import sys
import tokenize

PRAGMA = re.compile(r"#\s*pylint:")
PLAIN = re.compile(r"\s*#\s*pylint:.*")


def strip(path):
    with open(path, "rb") as handle:
        data = handle.read()
    try:
        encoding, _ = tokenize.detect_encoding(io.BytesIO(data).readline)
    except SyntaxError:
        encoding = "utf-8"
    text = data.decode(encoding, "surrogateescape")
    # readlines keeps each line's ending, so the rows are the tokenizer's rows.
    lines = io.StringIO(text, newline="").readlines()

    cut = {}  # row -> column where the pragma comment starts
    reached = 0  # rows whose tokens were all read
    failed = False
    source = iter(lines)
    try:
        for tok in tokenize.generate_tokens(lambda: next(source, "")):
            if tok.type == tokenize.COMMENT:
                found = PRAGMA.search(tok.string)
                if found:
                    row, col = tok.start
                    cut[row] = col + found.start()
            reached = tok.end[0]
    except (tokenize.TokenError, SyntaxError, UnicodeError):
        failed = True

    out = []
    for row, line in enumerate(lines, 1):
        body = line.rstrip("\r\n")
        ending = line[len(body) :]
        if row in cut:
            body = body[: cut[row]].rstrip()
        elif failed and row >= reached:
            body = PLAIN.sub("", body)
        out.append(body + ending)

    new = "".join(out).encode(encoding, "surrogateescape")
    if new != data:
        with open(path, "wb") as handle:
            handle.write(new)


if __name__ == "__main__":
    for path in sys.argv[1:]:
        strip(path)
