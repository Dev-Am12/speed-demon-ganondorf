#!/usr/bin/env python3
"""Generate large Track B stress datasets and their oracle hashes outside the repo."""
import argparse
import hashlib
import pathlib
import subprocess
import sys

ORG = pathlib.Path.home() / "sd-organizer"
OUT = pathlib.Path.home() / "sd-data" / "custom-b"


def write_case(name: str, docs: bytes, queries: bytes) -> None:
    case = OUT / name
    case.mkdir(parents=True, exist_ok=True)
    (case / "docs.txt").write_bytes(docs)
    (case / "queries.txt").write_bytes(queries)
    ref = case / "reference"
    ref.mkdir(exist_ok=True)
    subprocess.run(
        [sys.executable, str(ORG / "reference/b_index.py"), "--data", str(case), "--out", str(ref)],
        check=True,
    )
    expected = (ref / "results.txt").read_bytes()
    (case / "expected_results.txt").write_bytes(expected)
    digest = hashlib.sha256(expected).hexdigest()
    (case / "expected.sha256").write_text(digest + "\n", encoding="ascii")
    print(f"{name} {digest} {case}")


def term(i: int) -> bytes:
    # Bijective base-26 spelling, prefixed so every term starts with a letter.
    chars = []
    n = i
    while True:
        chars.append(chr(ord("a") + n % 26))
        n //= 26
        if n == 0:
            break
    return ("t" + "".join(reversed(chars))).encode("ascii")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("case", choices=("bigids", "vocab70k", "df-ladder", "all"), nargs="?", default="all")
    selected = parser.parse_args().case
    OUT.mkdir(parents=True, exist_ok=True)

    if selected in ("bigids", "all"):
        count = 120_000
        docs = b"".join(str(i).encode() + b"\tanchor\n" for i in range(count))
        write_case("bigids", docs, b"BOOL anchor\n")

    if selected in ("vocab70k", "all"):
        terms = [term(i) for i in range(70_000)]
        docs = b"0\t" + b" ".join(terms) + b"\n"
        queries = b"".join(b"BOOL " + item + b"\n" for item in terms)
        write_case("vocab70k", docs, queries)

    if selected in ("df-ladder", "all"):
        count = 70_000
        dfs = sorted({1, 2, 3, count // 32 - 1, count // 32, count // 32 + 1, count // 16})
        names = [b"df" + str(df).encode() for df in dfs]
        lines = []
        for doc_id in range(count):
            words = [name for name, df in zip(names, dfs) if doc_id < df]
            lines.append(str(doc_id).encode() + b"\t" + (b" ".join(words) if words else b"filler") + b"\n")
        docs = b"".join(lines)
        queries = b"".join(b"BOOL " + name + b"\n" for name in names)
        write_case("df-ladder", docs, queries)


if __name__ == "__main__":
    main()