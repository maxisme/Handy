# vocab-check results

Every full run in `results/`, oldest first. 62 cases, 3 runs each, on this machine. Regenerate with `python3 summarize_results.py --write`.

| prompt | schema | greedy | mode | context | precision | recall | agreement | p95 ms | median ms | FP | FN | parse fail | gate |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| v1 | bool | no | batch(4) | on | 0.94 | 0.82 | 0.77 | 1024 | 962 | 2 | 7 | 0 | FAIL |
| v2-examples | bool | no | batch(4) | on | 0.95 | 0.88 | 0.76 | 1093 | 1045 | 2 | 5 | 4 | FAIL |
| v1 | bool | no | batch(4) | off | 0.88 | 0.95 | 0.84 | 992 | 947 | 5 | 2 | 0 | FAIL |
| v1 | bool | yes | batch(4) | on | 0.89 | 0.97 | 1.00 | 979 | 936 | 5 | 1 | 0 | FAIL |
| v1 | bool | no | single(1) | on | 0.93 | 0.93 | 0.84 | 530 | 506 | 3 | 3 | 0 | FAIL |
| v3-kinds | kind | yes | batch(4) | on | 1.00 | 0.88 | 1.00 | 1210 | 1166 | 0 | 5 | 0 | PASS |
| v3-kinds | kind | no | batch(4) | on | 1.00 | 0.82 | 0.84 | 1216 | 1164 | 0 | 7 | 0 | FAIL |
| v3-kinds | bool | yes | batch(4) | on | 0.69 | 0.93 | 1.00 | 1111 | 1054 | 17 | 3 | 0 | FAIL |
| v4-kinds | kind | yes | batch(4) | on | 0.95 | 0.97 | 1.00 | 1242 | 1206 | 2 | 1 | 0 | PASS |
