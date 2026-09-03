# vocab-check

Measures whether Apple's on-device model can tell a vocabulary correction from a rewording. This is the judgment Handy's learn-from-corrections feature would hand to Apple Intelligence, so the prompt and the typed result here are the ones that go into `src-tauri/swift/apple_intelligence.swift` if the numbers are good enough.

Needs macOS 26 on Apple Silicon with Apple Intelligence turned on, and full Xcode (the Foundation Models macros are not in the command line tools).

## Run

```bash
swift run -c release vocab-check
```

Defaults: `cases.json`, `prompts/v1.txt`, batch mode with four pairs per call, sentence context on, three runs. The gate and thresholds are in `TEST-CASES.md`. Exit code is 0 when the gate passes, 1 when it fails, 2 when the model is unavailable.

Options:

```
--prompt <file>        prompt file (default prompts/v1.txt)
--mode batch|single    pairs per call: four, or one (default batch)
--batch-size <n>       pairs per call in batch mode (default 4)
--context on|off       include the sentence each pair came from (default on)
--runs <n>             repeats per case (default 3)
--seed <n>             shuffle seed for batching (default 42)
--greedy               greedy sampling instead of the model default
--schema bool|kind     ask for a yes/no per pair, or for the kind of correction and derive yes/no from it (default bool)
--prewarm              call prewarm() before the first request
--only <category>      run one category
--limit <n>            run the first n cases after shuffling
--verbose              print every verdict as it arrives
--min-precision <x>    gate (default 0.95)
--min-recall <x>       gate (default 0.80)
--min-agreement <x>    gate (default 0.95)
--max-p95-ms <n>       gate (default 1500)
--out <file>           results JSON path (default results/<timestamp>-<prompt>-<mode>-ctx<on|off>.json)
```

Compare two prompts:

```bash
swift run -c release vocab-check --prompt prompts/v1.txt
swift run -c release vocab-check --prompt prompts/v2-examples.txt
```

## Findings so far

Full table in `RESULTS.md`. Two configurations pass the gate on this machine:

| configuration                                 | precision | recall | agreement | p95   |
| --------------------------------------------- | --------- | ------ | --------- | ----- |
| `prompts/v3-kinds.txt --schema kind --greedy` | 1.00      | 0.88   | 1.00      | 1.2 s |
| `prompts/v4-kinds.txt --schema kind --greedy` | 0.95      | 0.97   | 1.00      | 1.2 s |

v3 is the one to ship. It never said yes to a rewording, and its misses are all false negatives the user can fix by adding the word by hand. v4 recovers most of those but starts calling synonyms vocabulary, which is the failure the feature cannot afford.

Three things the runs established:

- Asking for a boolean does not work. Even with the v3 prompt, the boolean schema said yes to nearly every rewording and grammar fix. Asking for the kind of correction and deriving yes or no in code is what fixed precision.
- Default sampling makes the answers unstable. Agreement was 0.76 to 0.84 across every sampled run. Greedy decoding gives 1.00 every time at no cost in accuracy. Handy must pass `GenerationOptions(sampling: .greedy)`.
- Sentence context matters. Without it, precision dropped to 0.88 on the same prompt.

## postprocess-probe

A second executable that replays Handy's Apple Intelligence post-processing on a few transcripts, using the prompt selected in the live settings store, and prints the result for several request styles side by side:

```bash
swift run -c release postprocess-probe "It costs fifty pounds a month."
```

What it showed on 3 Sep 2026. Rows are request shapes, columns are dictations; the prompt is the same in every cell.

| request shape                                                                       | "fifty pounds a month" | "twenty five tickets, um, ten percent" | "I just did a transcript … weird rewording" | "Please summarise this document" |
| ----------------------------------------------------------------------------------- | ---------------------- | -------------------------------------- | ------------------------------------------- | -------------------------------- |
| template as instructions, bare transcript as message, structured (Handy as shipped) | unchanged              | fillers removed, numbers unchanged     | unchanged                                   | **echoed the prompt back**       |
| same, plain text, greedy                                                            | converted              | converted                              | **answered it**                             | **answered it**                  |
| transcript wrapped in tags, plain, greedy                                           | **only "£50"**         | converted                              | unchanged                                   | **answered it**                  |
| **template with transcript inline as the message, structured, greedy**              | converted              | converted                              | unchanged                                   | unchanged                        |

Two lessons. The typed result is what stops the model treating a dictation as a request, so it stays. Putting the whole template in the user message, transcript included, is what makes the typed result actually apply the edits; with the template as instructions and an empty transcript slot, "the above is a transcript" points at nothing and the model copies. Greedy sampling keeps it consistent. This is now how `actions.rs` calls the bridge.

Even the best shape is a 3B model, so `actions.rs` also discards any output that no longer resembles the transcript (an answer, a summary, an extracted fragment) and pastes the raw text instead.

## Cases

`cases.json` is the source of truth. After editing it, regenerate the readable version:

```bash
python3 render_cases_md.py
```
