#!/usr/bin/env python3
"""Split document text into chunks for the build supervisor.

Uses chonkie when installed; falls back to paragraph boundaries so the
pipeline still runs on a machine with only Python 3.
"""

from __future__ import annotations

import json
import sys


def chunk(text: str, chunk_size: int = 2048) -> list[str]:
    text = text.strip()
    if not text:
        return []

    try:
        from chonkie import RecursiveChunker

        chunker = RecursiveChunker(chunk_size=chunk_size)
        return [c.text for c in chunker(text)]
    except ImportError:
        parts: list[str] = []
        buf: list[str] = []
        size = 0
        for para in text.split("\n\n"):
            para = para.strip()
            if not para:
                continue
            if size + len(para) > chunk_size and buf:
                parts.append("\n\n".join(buf))
                buf = []
                size = 0
            buf.append(para)
            size += len(para)
        if buf:
            parts.append("\n\n".join(buf))
        return parts


def main() -> None:
    raw = sys.stdin.read()
    size = 2048
    if len(sys.argv) > 1:
        try:
            size = int(sys.argv[1])
        except ValueError:
            pass
    json.dump({"chunks": chunk(raw, size)}, sys.stdout)


if __name__ == "__main__":
    main()
