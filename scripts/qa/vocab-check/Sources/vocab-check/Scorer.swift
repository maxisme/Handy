import Foundation

struct CaseOutcome: Codable {
    let id: String
    let category: String
    let heard: String
    let meant: String
    let expect: Bool
    /// One entry per run. nil means the call failed or did not parse.
    var answers: [Bool?]

    var majority: Bool {
        let yes = answers.filter { $0 == true }.count
        return yes * 2 > answers.count
    }
    var unanimous: Bool {
        guard let first = answers.first else { return true }
        return answers.allSatisfy { $0 == first }
    }
    var parseFailures: Int { answers.filter { $0 == nil }.count }
    var correct: Bool { majority == expect }
}

struct Latency: Codable {
    let firstCallMs: [Double]
    let medianMs: Double
    let p95Ms: Double
    let calls: Int
}

struct Metrics: Codable {
    let cases: Int
    let truePositives: Int
    let falsePositives: Int
    let falseNegatives: Int
    let trueNegatives: Int
    let precision: Double
    let recall: Double
    let agreement: Double
    let parseFailures: Int
    let meantMismatches: Int
    let latency: Latency
    let byCategory: [String: CategoryScore]
}

struct CategoryScore: Codable {
    let correct: Int
    let total: Int
}

struct Gate: Codable {
    let minPrecision: Double
    let minRecall: Double
    let minAgreement: Double
    let maxP95Ms: Double

    func failures(for m: Metrics) -> [String] {
        var out: [String] = []
        if m.precision < minPrecision { out.append(String(format: "precision %.2f < %.2f", m.precision, minPrecision)) }
        if m.recall < minRecall { out.append(String(format: "recall %.2f < %.2f", m.recall, minRecall)) }
        if m.agreement < minAgreement { out.append(String(format: "agreement %.2f < %.2f", m.agreement, minAgreement)) }
        if m.latency.p95Ms > maxP95Ms { out.append(String(format: "p95 %.0f ms > %.0f ms", m.latency.p95Ms, maxP95Ms)) }
        return out
    }
}

func percentile(_ sorted: [Double], _ p: Double) -> Double {
    guard !sorted.isEmpty else { return 0 }
    let rank = p * Double(sorted.count - 1)
    let low = Int(rank.rounded(.down))
    let high = Int(rank.rounded(.up))
    if low == high { return sorted[low] }
    let weight = rank - Double(low)
    return sorted[low] * (1 - weight) + sorted[high] * weight
}

func score(outcomes: [CaseOutcome], firstCalls: [Double], warmCalls: [Double], meantMismatches: Int) -> Metrics {
    var tp = 0, fp = 0, fn = 0, tn = 0
    for o in outcomes {
        switch (o.expect, o.majority) {
        case (true, true): tp += 1
        case (false, true): fp += 1
        case (true, false): fn += 1
        case (false, false): tn += 1
        }
    }
    let precision = tp + fp == 0 ? 1.0 : Double(tp) / Double(tp + fp)
    let recall = tp + fn == 0 ? 1.0 : Double(tp) / Double(tp + fn)
    let agreement = outcomes.isEmpty ? 1.0 : Double(outcomes.filter(\.unanimous).count) / Double(outcomes.count)

    var byCategory: [String: CategoryScore] = [:]
    for (category, group) in Dictionary(grouping: outcomes, by: \.category) {
        byCategory[category] = CategoryScore(correct: group.filter(\.correct).count, total: group.count)
    }

    let sortedWarm = warmCalls.sorted()
    let latency = Latency(
        firstCallMs: firstCalls,
        medianMs: percentile(sortedWarm, 0.5),
        p95Ms: percentile(sortedWarm, 0.95),
        calls: warmCalls.count + firstCalls.count
    )

    return Metrics(
        cases: outcomes.count,
        truePositives: tp,
        falsePositives: fp,
        falseNegatives: fn,
        trueNegatives: tn,
        precision: precision,
        recall: recall,
        agreement: agreement,
        parseFailures: outcomes.reduce(0) { $0 + $1.parseFailures },
        meantMismatches: meantMismatches,
        latency: latency,
        byCategory: byCategory
    )
}
