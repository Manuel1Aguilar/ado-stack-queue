#!/usr/bin/env python3
"""Copy lint for text outside readers see (open-source READMEs first).

Regex check for the tells in Manu's user-facing-copy skill
(~factory/.claude/skills/user-facing-copy/SKILL.md). Each hit forces a
rewrite. Code blocks and inline code are skipped. Exit 0 = clean, 1 = hits.
Usage: copy-lint.py FILE...
"""
import re
import sys

RULES = [
    ("negative parallelism", r"\bnot (just|only|merely)\b|\brather than\b"),
    ("split contrast", r"\b(isn't|is not|wasn't|was not|aren't|weren't)\s[^.!?\n]{1,40}[.!?]\s+[A-Z][^.!?\n]{0,40}\b(is|was|are|were)\b"),
    ("self-answered question", r"(^|[.!?]\s+)(The (secret|answer|catch|trick|result)|Why|How)\?\s"),
    ("false suspense", r"\b(here's the (kicker|thing|catch|twist)|here's where it gets|let that sink in)\b"),
    ("-ing benefit clause", r",\s(ensuring|helping|making it|allowing|enabling|empowering|letting you)\b[^.\n]*\."),
    ("chat residue", r"\b(certainly!|of course!|great question|here's a breakdown|let me know if|i hope this helps)"),
    ("oops", r"\b(oops|uh-oh|whoops)\b"),
    ("vocabulary", r"\b(seamless(ly)?|effortless(ly)?|elevate[sd]?|unlock(s|ed)?|journey|insights|comprehensive|delve|"
                   r"crucial|pivotal|underscore[sd]?|showcase[sd]?|enhance[sd]?|multifaceted|cutting-edge|"
                   r"best-in-class|industry-leading|committed to|leverag(e|es|ed|ing)|utili[sz]e[sd]?|robust|"
                   r"streamlin(e|es|ed|ing)|empower(s|ed|ing)?|facilitat(e|es|ed|ing)|foster(s|ed|ing)?|"
                   r"going forward|in order to|game-?chang(er|ing))\b"),
    ("magic adverb", r"\b(quietly|deeply|fundamentally|remarkably|truly)\b"),
    ("copula avoidance", r"\b(serves as|stands as|functions as|boasts)\b"),
    ("bold-colon bullet", r"^\s*[-*]\s+\*\*[^*]+:\*\*"),
    ("exclamation", r"[A-Za-z)]!(\s|$)"),
]
EM_DASH_BUDGET = 2


def prose(text):
    text = re.sub(r"```.*?```", lambda m: "\n" * m.group(0).count("\n"), text, flags=re.S)
    return re.sub(r"`[^`\n]*`", "``", text)


def lint(path):
    text = prose(open(path, encoding="utf-8").read())
    hits = []
    for n, line in enumerate(text.splitlines(), 1):
        for name, rx in RULES:
            for m in re.finditer(rx, line, re.I if name not in ("split contrast", "self-answered question") else 0):
                hits.append(f"{path}:{n}: {name}: {m.group(0).strip()[:60]!r}")
    dashes = text.count(" — ") + text.count(" – ")
    if dashes > EM_DASH_BUDGET:
        hits.append(f"{path}: {dashes} spaced dashes (budget {EM_DASH_BUDGET}); use a comma, colon or full stop")
    return hits


if __name__ == "__main__":
    found = [h for p in sys.argv[1:] for h in lint(p)]
    print("\n".join(found) or "copy-lint: clean")
    sys.exit(1 if found else 0)
