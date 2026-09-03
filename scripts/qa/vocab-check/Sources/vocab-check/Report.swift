import Foundation

struct RunConfig: Codable {
    let prompt: String
    let mode: String
    let batchSize: Int
    let context: Bool
    let runs: Int
    let seed: UInt64
    let greedy: Bool
    let prewarm: Bool
    let schema: String
    let only: String?
    let limit: Int?
    let startedAt: String
    let machine: String
}

struct ResultsFile: Codable {
    let config: RunConfig
    let gate: Gate
    let gateFailures: [String]
    let metrics: Metrics
    let outcomes: [CaseOutcome]
    let callErrors: [String]
}

func printReport(_ results: ResultsFile) {
    let m = results.metrics
    func pct(_ x: Double) -> String { String(format: "%.2f", x) }

    print("")
    print("Results  cases=\(m.cases)  runs=\(results.config.runs)  schema=\(results.config.schema)  greedy=\(results.config.greedy)  mode=\(results.config.mode)(\(results.config.batchSize))  context=\(results.config.context ? "on" : "off")  prompt=\(results.config.prompt)")
    print("  precision \(pct(m.precision)) (\(m.truePositives)/\(m.truePositives + m.falsePositives))   recall \(pct(m.recall)) (\(m.truePositives)/\(m.truePositives + m.falseNegatives))   agreement \(pct(m.agreement))")
    print("  confusion  TP \(m.truePositives)  FP \(m.falsePositives)  FN \(m.falseNegatives)  TN \(m.trueNegatives)   parse failures \(m.parseFailures)   meant mismatches \(m.meantMismatches)")
    let firsts = m.latency.firstCallMs.map { String(format: "%.0f", $0) }.joined(separator: ", ")
    print("  latency    first call per run \(firsts) ms · median \(String(format: "%.0f", m.latency.medianMs)) ms · p95 \(String(format: "%.0f", m.latency.p95Ms)) ms  (\(m.latency.calls) calls)")

    print("  by category")
    for key in m.byCategory.keys.sorted() {
        let c = m.byCategory[key]!
        let flag = c.correct == c.total ? "" : "  ←"
        print("    \(key.padding(toLength: 12, withPad: " ", startingAt: 0)) \(c.correct)/\(c.total)\(flag)")
    }

    let misses = results.outcomes.filter { !$0.correct }
    if !misses.isEmpty {
        print("  misses")
        for o in misses {
            let answers = o.answers.map { $0 == nil ? "?" : ($0! ? "yes" : "no") }.joined(separator: " ")
            print("    \(o.id.padding(toLength: 10, withPad: " ", startingAt: 0)) expect \(o.expect ? "yes" : "no ")  got [\(answers)]   \"\(o.heard)\" → \"\(o.meant)\"")
        }
    }
    let unstable = results.outcomes.filter { !$0.unanimous && $0.correct }
    if !unstable.isEmpty {
        print("  unstable but correct on majority")
        for o in unstable {
            let answers = o.answers.map { $0 == nil ? "?" : ($0! ? "yes" : "no") }.joined(separator: " ")
            print("    \(o.id.padding(toLength: 10, withPad: " ", startingAt: 0)) [\(answers)]   \"\(o.heard)\" → \"\(o.meant)\"")
        }
    }
    if !results.callErrors.isEmpty {
        print("  call errors (\(results.callErrors.count))")
        for e in results.callErrors.prefix(5) { print("    \(e)") }
    }

    print("")
    if results.gateFailures.isEmpty {
        print("Gate: PASS")
    } else {
        print("Gate: FAIL  " + results.gateFailures.joined(separator: "; "))
    }
}

func writeResults(_ results: ResultsFile, to path: String) throws {
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    let data = try encoder.encode(results)
    let url = URL(fileURLWithPath: path)
    try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    try data.write(to: url)
}
