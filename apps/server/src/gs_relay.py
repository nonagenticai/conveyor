#!/usr/bin/env python3
"""Ghostscript relay for the conveyor benchmark sandbox.

The instance images ship GPL Ghostscript 9.55 whose SAFER policy refuses every
read and write under ``/testbed``: ``--permit-file-read``/``--permit-file-write``
are ineffective in that build and ``-dNOSAFER`` is ignored, while ``/tmp`` is
allowed. matplotlib's PDF/EPS image comparison drives one interactive
Ghostscript process with PostScript lines of the form::

    << /OutputFile (DEST) >> setpagedevice (SRC) run flush

so this relay mirrors every ``/testbed`` path named in that stream under a
private ``/tmp`` root, runs the real Ghostscript against the mirror, and copies
the produced files back to their real paths *before* it forwards Ghostscript's
response (matplotlib checks the destination exists as soon as it reads the
prompt).

Invocations that are not the interactive protocol (matplotlib's
``gs --version`` probe, or any call with a file operand or ``-c``) are exec'd
unchanged: Ghostscript is the only authority on them.
"""

import os
import re
import shutil
import subprocess
import sys
import tempfile

GS = "/usr/bin/gs"
ROOT = "/testbed"
# The end of an interactive Ghostscript prompt: "GS>" or "GS<n>".
PROMPT = re.compile(rb"GS(?:<\d+>|>)$")
# A PostScript string literal, honouring the three escapes matplotlib emits.
LITERAL = re.compile(r"\((?:\\.|[^\\()])*\)")
# The output file in matplotlib's `<< /OutputFile (DEST) >> setpagedevice` command.
OUTPUT = re.compile(r"/OutputFile\s*\((?:\\.|[^\\)])*\)")


def unescape(text):
    """Undo Ghostscript string escapes: \\\\, \\( and \\)."""
    out = []
    index = 0
    while index < len(text):
        if text[index] == "\\" and index + 1 < len(text):
            out.append(text[index + 1])
            index += 2
        else:
            out.append(text[index])
            index += 1
    return "".join(out)


def escape(text):
    """Apply Ghostscript string escapes: \\\\, \\( and \\)."""
    return text.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")


def stage_path(stage, path):
    """Return the mirror of a /testbed path under the private stage root."""
    return stage + path


def main():
    args = sys.argv[1:]
    interactive = not (
        any(arg in ("--version", "-h", "-?", "--help", "-c", "-f") for arg in args)
        or any(not arg.startswith("-") for arg in args)
    )
    if not interactive:
        os.execv(GS, [GS, *args])

    stage = tempfile.mkdtemp(prefix="bench-gs-")
    child = subprocess.Popen(
        [GS, *args],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=None,
        bufsize=0,
    )

    def rewrite(line):
        """Mirror every /testbed literal; collect the paths that are outputs."""
        # A destination may already exist from an earlier comparison, so existence alone
        # cannot tell an input from an output: `/OutputFile (...)` is authoritative.
        declared = {
            unescape(match.group(0)[match.group(0).index("(") + 1 : -1])
            for match in OUTPUT.finditer(line)
        }
        outputs = []

        def replace(match):
            value = unescape(match.group(0)[1:-1])
            if not value.startswith(ROOT + "/"):
                return match.group(0)
            staged = stage_path(stage, value)
            os.makedirs(os.path.dirname(staged), exist_ok=True)
            if value in declared:
                outputs.append((staged, value))
            elif os.path.isfile(value):
                shutil.copyfile(value, staged)
            return "(" + escape(staged) + ")"

        return LITERAL.sub(replace, line), outputs

    def read_until_prompt():
        """Read Ghostscript's reply, ending at the interactive prompt."""
        buffer = bytearray()
        while True:
            byte = child.stdout.read(1)
            if not byte:
                return bytes(buffer), True
            buffer.extend(byte)
            if PROMPT.search(buffer):
                return bytes(buffer), False

    def emit(buffer, outputs):
        """Copy the produced files back, then forward Ghostscript's reply."""
        for staged, original in outputs:
            if os.path.isfile(staged):
                os.makedirs(os.path.dirname(original), exist_ok=True)
                shutil.copyfile(staged, original)
        sys.stdout.buffer.write(buffer)
        sys.stdout.buffer.flush()

    banner, eof = read_until_prompt()
    emit(banner, [])
    if eof:
        child.wait()
        return child.returncode

    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            break
        text, outputs = rewrite(line.decode("utf-8", "surrogateescape"))
        try:
            child.stdin.write(text.encode("utf-8", "surrogateescape"))
            child.stdin.flush()
        except BrokenPipeError:
            break
        reply, eof = read_until_prompt()
        emit(reply, outputs)
        if eof:
            break

    try:
        child.stdin.close()
    except BrokenPipeError:
        pass
    rest = child.stdout.read()
    if rest:
        sys.stdout.buffer.write(rest)
        sys.stdout.buffer.flush()
    child.wait()
    return child.returncode


if __name__ == "__main__":
    sys.exit(main())
