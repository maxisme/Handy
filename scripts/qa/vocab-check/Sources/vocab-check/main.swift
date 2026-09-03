import Foundation

struct Options {
    var cases = "cases.json"
    var prompt = "prompts/v1.txt"
    var mode = "batch"
    var batchSize = 4
    var context = true
    var runs = 3
    var seed: UInt64 = 42
    var greedy = false
    var prewarm = false
    var schema = Schema.bool
    var only: String? = nil
    var limit: Int? = nil
    var verbose = false
    var gate = Gate(minPrecision: 0.95, minRecall: 0.80, minAgreement: 0.95, maxP95Ms: 1500)
    var out: String? = nil
}

func parseOptions() -> Options {
    var o = Options()
    var args = Array(CommandLine.arguments.dropFirst())
    func take() -> String {
        guard !args.isEmpty else { fail("missing value for option") }
        return args.removeFirst()
    }
    func takeInt(_ option: String) -> Int {
        guard let v = Int(take()) else { fail("\(option) needs a whole number") }
        return v
    }
    func takeDouble(_ option: String) -> Double {
        guard let v = Double(take()) else { fail("\(option) needs a number") }
        return v
    }
    while !args.isEmpty {
        let a = args.removeFirst()
        switch a {
        case "--cases": o.cases = take()
        case "--prompt": o.prompt = take()
        case "--mode":
            o.mode = take()
            guard o.mode == "batch" || o.mode == "single" else { fail("--mode must be batch or single") }
        case "--batch-size": o.batchSize = takeInt("--batch-size")
        case "--context":
            let v = take()
            guard v == "on" || v == "off" else { fail("--context must be on or off") }
            o.context = v == "on"
        case "--runs": o.runs = takeInt("--runs")
        case "--seed": o.seed = UInt64(takeInt("--seed"))
        case "--greedy": o.greedy = true
        case "--schema":
            guard let v = Schema(rawValue: take()) else { fail("--schema must be bool or kind") }
            o.schema = v
        case "--prewarm": o.prewarm = true
        case "--only": o.only = take()
        case "--limit": o.limit = takeInt("--limit")
        case "--verbose": o.verbose = true
        case "--min-precision": o.gate = Gate(minPrecision: takeDouble("--min-precision"), minRecall: o.gate.minRecall, minAgreement: o.gate.minAgreement, maxP95Ms: o.gate.maxP95Ms)
        case "--min-recall": o.gate = Gate(minPrecision: o.gate.minPrecision, minRecall: takeDouble("--min-recall"), minAgreement: o.gate.minAgreement, maxP95Ms: o.gate.maxP95Ms)
        case "--min-agreement": o.gate = Gate(minPrecision: o.gate.minPrecision, minRecall: o.gate.minRecall, minAgreement: takeDouble("--min-agreement"), maxP95Ms: o.gate.maxP95Ms)
        case "--max-p95-ms": o.gate = Gate(minPrecision: o.gate.minPrecision, minRecall: o.gate.minRecall, minAgreement: o.gate.minAgreement, maxP95Ms: takeDouble("--max-p95-ms"))
        case "--out": o.out = take()
        case "-h", "--help":
            print("See README.md for options.")
            exit(0)
        default:
            fail("unknown option \(a)")
        }
    }
    return o
}

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(("error: " + message + "\n").data(using: .utf8)!)
    exit(2)
}

let options = parseOptions()

let instructions: String
do {
    instructions = try String(contentsOfFile: options.prompt, encoding: .utf8)
} catch {
    fail("cannot read prompt file \(options.prompt): \(error)")
}

var cases: [TestCase]
do {
    cases = try loadCases(from: options.cases)
} catch {
    fail("cannot read cases file \(options.cases): \(error)")
}
if let only = options.only {
    cases = cases.filter { $0.category == only }
    if cases.isEmpty { fail("no cases in category \(only)") }
}

var generator = SeededGenerator(seed: options.seed)
cases.shuffle(using: &generator)
if let limit = options.limit {
    cases = Array(cases.prefix(limit))
}

let runner = Runner(instructions: instructions, greedy: options.greedy, schema: options.schema)
if let problem = runner.availabilityProblem() {
    fail(problem)
}

let groupSize = options.mode == "single" ? 1 : max(1, options.batchSize)
let groups: [[TestCase]] = stride(from: 0, to: cases.count, by: groupSize).map {
    Array(cases[$0..<min($0 + groupSize, cases.count)])
}

let promptName = URL(fileURLWithPath: options.prompt).deletingPathExtension().lastPathComponent
print("vocab-check  prompt=\(promptName)  schema=\(options.schema.rawValue)  greedy=\(options.greedy)  mode=\(options.mode)(\(groupSize))  context=\(options.context ? "on" : "off")  runs=\(options.runs)  cases=\(cases.count)  calls/run=\(groups.count)")

var outcomes: [String: CaseOutcome] = [:]
for c in cases {
    outcomes[c.id] = CaseOutcome(id: c.id, category: c.category, heard: c.heard, meant: c.meant, expect: c.expect, answers: [])
}
var firstCalls: [Double] = []
var warmCalls: [Double] = []
var callErrors: [String] = []
var meantMismatches = 0

if options.prewarm {
    await runner.prewarm()
}

for run in 1...max(1, options.runs) {
    print("run \(run)/\(options.runs) ", terminator: "")
    for (index, group) in groups.enumerated() {
        let result = await runner.call(pairs: group, withContext: options.context)
        if index == 0 { firstCalls.append(result.milliseconds) } else { warmCalls.append(result.milliseconds) }
        meantMismatches += result.meantMismatches
        if let error = result.error {
            callErrors.append("run \(run) call \(index + 1): \(error)")
            print("x", terminator: "")
            for c in group { outcomes[c.id]!.answers.append(nil) }
        } else {
            print(".", terminator: "")
            for (i, c) in group.enumerated() {
                let answer = result.verdicts![i]
                outcomes[c.id]!.answers.append(answer)
                if options.verbose {
                    let mark = answer == c.expect ? "ok " : "MISS"
                    print("\n  \(mark) \(c.id) \(answer ? "yes" : "no") \"\(c.heard)\" → \"\(c.meant)\"  \(String(format: "%.0f", result.milliseconds)) ms", terminator: "")
                }
            }
        }
        fflush(stdout)
    }
    print("")
}

let orderedOutcomes = cases.map { outcomes[$0.id]! }.sorted { $0.id < $1.id }
let metrics = score(outcomes: orderedOutcomes, firstCalls: firstCalls, warmCalls: warmCalls, meantMismatches: meantMismatches)
let gateFailures = options.gate.failures(for: metrics)

let stamp: String = {
    let f = DateFormatter()
    f.dateFormat = "yyyyMMdd-HHmmss"
    return f.string(from: Date())
}()
let machine = ProcessInfo.processInfo.operatingSystemVersionString
let config = RunConfig(
    prompt: promptName, mode: options.mode, batchSize: groupSize, context: options.context, runs: options.runs,
    seed: options.seed, greedy: options.greedy, prewarm: options.prewarm, schema: options.schema.rawValue, only: options.only, limit: options.limit,
    startedAt: stamp, machine: machine
)
let results = ResultsFile(config: config, gate: options.gate, gateFailures: gateFailures, metrics: metrics, outcomes: orderedOutcomes, callErrors: callErrors)

printReport(results)

let outPath = options.out ?? "results/\(stamp)-\(promptName)-\(options.mode)-ctx\(options.context ? "on" : "off").json"
do {
    try writeResults(results, to: outPath)
    print("results written to \(outPath)")
} catch {
    FileHandle.standardError.write("warning: could not write results: \(error)\n".data(using: .utf8)!)
}

exit(gateFailures.isEmpty ? 0 : 1)
