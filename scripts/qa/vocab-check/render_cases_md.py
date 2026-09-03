#!/usr/bin/env python3
"""Render TEST-CASES.md from cases.json. Run after editing cases.json."""
import json
from collections import OrderedDict
from pathlib import Path

HERE = Path(__file__).parent
cases = json.loads((HERE / "cases.json").read_text())["cases"]

CATEGORY_TITLES = OrderedDict([
    ("product", "Product names"),
    ("project", "Project and service names (made up)"),
    ("acronym", "Acronyms and initialisms"),
    ("person", "People's names"),
    ("technical", "Technical terms"),
    ("rewording", "Rewording and synonyms"),
    ("grammar", "Grammar, tense, homophones, plurals"),
    ("near-miss", "Common words misheard as other common words"),
    ("formatting", "Numbers and currency"),
])

positives = sum(1 for c in cases if c["expect"])
negatives = len(cases) - positives

out = []
out.append("# Vocabulary check test cases\n")
out.append(f"{len(cases)} labelled pairs: {positives} the model should say **yes** to, {negatives} it should say **no** to. "
           "Each pair is what the speech model wrote, what the user changed it to, and the sentence it sat in. "
           "Generated from `cases.json` by `render_cases_md.py`; edit the JSON, not this file.\n")

out.append("## What the script asserts\n")
out.append("Every case is sent to the on-device Apple model three times. A case's verdict is the majority of its three answers. "
           "A call that errors, times out, or returns something that does not fit the typed result counts as **no** for that case and is reported as a parse failure.\n")
out.append("### Gate (the run fails if any of these miss)\n")
out.append("| Metric | Threshold | Why this number |\n|---|---|---|")
out.append("| Precision | ≥ 0.95 | A false yes puts junk in the user's dictionary. They have to notice and remove it. |")
out.append("| Recall | ≥ 0.80 | A false no costs nothing. The user adds the word by hand. |")
out.append("| Agreement | ≥ 0.95 | Share of cases where all three runs gave the same answer. Below this the model is guessing. |")
out.append("| p95 latency | ≤ 1500 ms | Per call, batch of four pairs. Sets the gap between saving an edit and seeing the toast. |\n")
out.append("### Reported but not gated\n")
out.append("- First call of each run, on its own. That is model load time, paid once per app session.")
out.append("- Median latency.")
out.append("- Accuracy per category, so a weak category is visible even when the overall gate passes.")
out.append("- Every case that missed, with what the model said on each run.")
out.append("- Count of responses whose `meant` text did not match the input, which would mean the model reordered or rewrote pairs.\n")
out.append("### Variants the script can compare\n")
out.append("- Prompt file: `prompts/v1.txt` (rules only), `prompts/v2-examples.txt` (rules plus five examples), or `prompts/v3-kinds.txt` (asks for the kind of correction).")
out.append("- Result schema: `bool` asks the model for yes or no per pair; `kind` asks it to name the kind of correction and the script derives yes or no. Names, products, projects, acronyms and technical terms are yes; common words, rewording, grammar and formatting are no.")
out.append("- Sentence context on or off.")
out.append("- Batch mode (four pairs per call, shuffled with a fixed seed so batches mix yes and no) or single (one pair per call).")
out.append("- Greedy sampling on or off. Default is the model's own sampling, which is what Handy would ship with.\n")

out.append("## Cases\n")
for cat, title in CATEGORY_TITLES.items():
    rows = [c for c in cases if c["category"] == cat]
    if not rows:
        continue
    exp = "yes" if rows[0]["expect"] else "no"
    out.append(f"### {title}\n")
    out.append(f"Expected answer for every row: **{exp}**.\n")
    out.append("| id | heard | meant | sentence | note |\n|---|---|---|---|---|")
    for c in rows:
        note = c.get("note", "")
        out.append(f"| {c['id']} | {c['heard']} | {c['meant']} | {c['context']} | {note} |")
    out.append("")

(HERE / "TEST-CASES.md").write_text("\n".join(out))
print(f"wrote TEST-CASES.md ({len(cases)} cases)")
