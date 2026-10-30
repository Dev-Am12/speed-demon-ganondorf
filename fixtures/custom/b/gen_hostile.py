#!/usr/bin/env python3
"""Write small committed Track B hostile fixtures; expected output is always the organizer reference."""
from pathlib import Path
import hashlib
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]
FIXTURES = Path(__file__).resolve().parent
ORG = Path.home() / "sd-organizer"
ORACLE_TMP = Path.home() / "sd-data" / "custom-b" / "oracle"

CASES = {
    "encoding_tokens": (
        ["Zürich bail—bond NAÏVE", "CAFÉ 007", "BAIL BOND", "a\u00a0b"],
        ["BOOL z", "BOOL rich", "BOOL zurich", "BOOL 007", "BOOL 7", "BOOL na", "BOOL ve", "BOOL bail AND bond", "PHRASE \"bail bond\"", "PHRASE \"a b\""],
        "ASCII case folding, non-ASCII separators, punctuation, NBSP, and 007 versus 7.",
    ),
    "empty_positions": (
        ["", "the", "", "bail", ""],
        ["BOOL the", "BOOL bail", "PHRASE \"the bail\"", "BOOL missing"],
        "Empty texts at the beginning, middle, and end of the corpus.",
    ),
    "phrase_edges": (
        ["bail bail bail bond", "bail bond bail bond", "estate. bond", "end", "start"],
        ["PHRASE \"bail bail bond\"", "PHRASE \"bail bond\"", "PHRASE \"estate bond\"", "PHRASE \"end start\""],
        "Repeated phrase lookahead, punctuation adjacency, sentence crossing, and no cross-document phrase.",
    ),
    "token_lengths": (
        ["a " + "x" * 33 + " b", "a " + "x" * 32 + " b", "x" * 32],
        ["PHRASE \"a b\"", "PHRASE \"a " + "x" * 32 + " b\"", "BOOL " + "x" * 32],
        "Overlong tokens occupy a phrase position; 32-byte terms remain matchable.",
    ),
    "operators_absent": (
        ["and or not alpha beta", "alpha beta", "and alpha", ""],
        ["BOOL and", "BOOL or", "BOOL not", "BOOL alpha NOT beta", "BOOL alpha NOT beta NOT alpha", "BOOL alpha NOT alpha", "BOOL absent", "BOOL alpha OR absent"],
        "Lowercase operator words as terms, left-associative NOT chains, self-difference, and absent terms.",
    ),
    "result_limits_duplicates": (
        ["ten eleven over" if i < 10 else ("eleven over" if i == 10 else "over") for i in range(12)],
        ["BOOL ten", "BOOL eleven", "BOOL over", "BOOL over", "PHRASE \"ten eleven\""],
        "Exactly 10, exactly 11, and more than 10 results, plus duplicate query lines.",
    ),
    "docs_1": (["solo"], ["BOOL solo", "BOOL absent"], "One-document corpus."),
    "docs_2": (["left", "right"], ["BOOL left OR right", "PHRASE \"left right\""], "Two-document corpus and cross-document phrase rejection."),
    "docs_5": (["one", "two", "three", "four", "five"], ["BOOL one OR five", "BOOL absent"], "Five-document corpus, fewer documents than the thread cap."),
}


def main() -> None:
    ORACLE_TMP.mkdir(parents=True, exist_ok=True)
    for name, (texts, queries, note) in CASES.items():
        case = FIXTURES / name
        case.mkdir(parents=True, exist_ok=True)
        docs = b"".join(str(i).encode("ascii") + b"\t" + text.encode("utf-8") + b"\n" for i, text in enumerate(texts))
        query_bytes = ("\n".join(queries) + "\n").encode("ascii")
        (case / "docs.txt").write_bytes(docs)
        (case / "queries.txt").write_bytes(query_bytes)
        (case / "README.md").write_text(note + "\n", encoding="utf-8")
        with tempfile.TemporaryDirectory(prefix="b-oracle-", dir=ORACLE_TMP) as temp:
            subprocess.run([sys.executable, str(ORG / "reference/b_index.py"), "--data", str(case), "--out", temp], check=True)
            expected = (Path(temp) / "results.txt").read_bytes()
        (case / "expected_results.txt").write_bytes(expected)
        (case / "expected.sha256").write_text(hashlib.sha256(expected).hexdigest() + "\n", encoding="ascii")
        print(f"wrote {name}")


if __name__ == "__main__":
    main()