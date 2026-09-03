# Vocabulary check test cases

62 labelled pairs: 40 the model should say **yes** to, 22 it should say **no** to. Each pair is what the speech model wrote, what the user changed it to, and the sentence it sat in. Generated from `cases.json` by `render_cases_md.py`; edit the JSON, not this file.

## What the script asserts

Every case is sent to the on-device Apple model three times. A case's verdict is the majority of its three answers. A call that errors, times out, or returns something that does not fit the typed result counts as **no** for that case and is reported as a parse failure.

### Gate (the run fails if any of these miss)

| Metric | Threshold | Why this number |
|---|---|---|
| Precision | ≥ 0.95 | A false yes puts junk in the user's dictionary. They have to notice and remove it. |
| Recall | ≥ 0.80 | A false no costs nothing. The user adds the word by hand. |
| Agreement | ≥ 0.95 | Share of cases where all three runs gave the same answer. Below this the model is guessing. |
| p95 latency | ≤ 1500 ms | Per call, batch of four pairs. Sets the gap between saving an edit and seeing the toast. |

### Reported but not gated

- First call of each run, on its own. That is model load time, paid once per app session.
- Median latency.
- Accuracy per category, so a weak category is visible even when the overall gate passes.
- Every case that missed, with what the model said on each run.
- Count of responses whose `meant` text did not match the input, which would mean the model reordered or rewrote pairs.

### Variants the script can compare

- Prompt file: `prompts/v1.txt` (rules only), `prompts/v2-examples.txt` (rules plus five examples), or `prompts/v3-kinds.txt` (asks for the kind of correction).
- Result schema: `bool` asks the model for yes or no per pair; `kind` asks it to name the kind of correction and the script derives yes or no. Names, products, projects, acronyms and technical terms are yes; common words, rewording, grammar and formatting are no.
- Sentence context on or off.
- Batch mode (four pairs per call, shuffled with a fixed seed so batches mix yes and no) or single (one pair per call).
- Greedy sampling on or off. Default is the model's own sampling, which is what Handy would ship with.

## Cases

### Product names

Expected answer for every row: **yes**.

| id | heard | meant | sentence | note |
|---|---|---|---|---|
| prod-01 | Charge B | ChargeBee | We moved billing over to Charge B last week. |  |
| prod-02 | cooper netties | Kubernetes | We run everything on cooper netties now. |  |
| prod-03 | post grass | Postgres | The data lives in post grass. |  |
| prod-04 | fig ma | Figma | The designs are in fig ma. |  |
| prod-05 | cloud flair | Cloudflare | DNS is on cloud flair. |  |
| prod-06 | zap ear | Zapier | Hook it up with zap ear. |  |
| prod-07 | git hub | GitHub | Push it to git hub before lunch. |  |
| prod-08 | mac book | MacBook | My mac book needs a restart. |  |
| prod-09 | chat gee pee tee | ChatGPT | Ask chat gee pee tee to draft it. |  |
| prod-10 | tail wind | Tailwind | The buttons use tail wind classes. |  |

### Project and service names (made up)

Expected answer for every row: **yes**.

| id | heard | meant | sentence | note |
|---|---|---|---|---|
| name-01 | by frost | Bifrost | The staging environment is called by frost. |  |
| name-02 | zen tricks | Zentrix | Move the zen tricks data before Friday. |  |
| name-03 | kah voo | Kavuu | Load it into kah voo. |  |
| name-04 | lottie h q | LottieHQ | Send it from the lottie h q account. |  |
| name-05 | oh strava | Ostrava | The oh strava service handles enquiries. |  |
| name-06 | no kia | Nokia | The no kia repo owns the listings API. |  |

### Acronyms and initialisms

Expected answer for every row: **yes**.

| id | heard | meant | sentence | note |
|---|---|---|---|---|
| acr-01 | R and D | R&D | Send the report to the R and D team. |  |
| acr-02 | a p i | API | The a p i returns JSON. |  |
| acr-03 | s d k | SDK | Update the s d k first. |  |
| acr-04 | c i c d | CI/CD | The c i c d pipeline is red. |  |
| acr-05 | g d p r | GDPR | We need a g d p r review. |  |
| acr-06 | o k rs | OKRs | Draft the o k rs for next quarter. |  |
| acr-07 | jot | JWT | The jot expires after an hour. | heard is a real word; needs the sentence to decide |
| acr-08 | s l a | SLA | That breaks the s l a. |  |
| acr-09 | sequel | SQL | Write the sequel query. | heard is a real word |

### People's names

Expected answer for every row: **yes**.

| id | heard | meant | sentence | note |
|---|---|---|---|---|
| person-01 | pree yanka | Priyanka | Ask pree yanka to review it. |  |
| person-02 | shiv on | Siobhan | shiv on is out until Monday. |  |
| person-03 | shao when | Xiaowen | Loop in shao when from data. |  |
| person-04 | n gozi | Ngozi | n gozi owns that service. |  |
| person-05 | soren | Søren | soren wrote the migration. | diacritic only, but a name |
| person-06 | ta deush | Tadeusz | Pair with ta deush on it. |  |

### Technical terms

Expected answer for every row: **yes**.

| id | heard | meant | sentence | note |
|---|---|---|---|---|
| tech-01 | I dem potent | idempotent | Make the endpoint I dem potent. |  |
| tech-02 | memo eyes ation | memoization | Add memo eyes ation to the parser. |  |
| tech-03 | web hook | webhook | Fire a web hook on save. |  |
| tech-04 | mono repo | monorepo | Everything is in the mono repo. |  |
| tech-05 | terror form | Terraform | Apply the terror form changes. |  |
| tech-06 | back fill | backfill | Run the back fill overnight. |  |
| tech-07 | mew tex | mutex | Guard it with a mew tex. |  |
| tech-08 | pie thon | Python | The script is written in pie thon. |  |
| tech-09 | cash | cache | Clear the cash and retry. | both real words; technical term wins on context |

### Rewording and synonyms

Expected answer for every row: **no**.

| id | heard | meant | sentence | note |
|---|---|---|---|---|
| reword-01 | went to | walked to | I went to the office early. |  |
| reword-02 | very big | huge | That is a very big change. |  |
| reword-03 | I think that | I believe | I think that we should ship it. |  |
| reword-04 | get | obtain | We need to get approval first. |  |
| reword-05 | quickly | fast | It runs quickly enough. |  |
| reword-06 | a lot of | many | There are a lot of edge cases. |  |
| reword-07 | help | assist | Can you help with the rollout? |  |
| reword-08 | start | begin | Let's start the migration on Monday. |  |

### Grammar, tense, homophones, plurals

Expected answer for every row: **no**.

| id | heard | meant | sentence | note |
|---|---|---|---|---|
| gram-01 | go | went | Yesterday I go to the dentist. |  |
| gram-02 | is | are | The results is ready. |  |
| gram-03 | run | ran | The tests run fine last night. |  |
| gram-04 | their | there | Put it over their. | homophone |
| gram-05 | its | it's | I think its going to rain. |  |
| gram-06 | report | reports | Send me the report by Friday. | plural of a common word |

### Common words misheard as other common words

Expected answer for every row: **no**.

| id | heard | meant | sentence | note |
|---|---|---|---|---|
| typo-01 | form | from | The email form the vendor arrived. |  |
| typo-02 | than | then | First build, than test. |  |
| typo-03 | to | too | That is to slow. |  |
| typo-04 | of | off | Turn it of before you leave. |  |
| typo-05 | the sauce | the source | Check the sauce code first. | mishearing of a common word, not vocabulary |

### Numbers and currency

Expected answer for every row: **no**.

| id | heard | meant | sentence | note |
|---|---|---|---|---|
| fmt-01 | twenty five | 25 | There are twenty five open tickets. |  |
| fmt-02 | ten percent | 10% | Usage grew ten percent. |  |
| fmt-03 | five dollars | $5 | It costs five dollars a month. |  |
