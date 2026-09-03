import Foundation

struct CaseFile: Codable {
    let version: Int
    let cases: [TestCase]
}

struct TestCase: Codable {
    let id: String
    let category: String
    let heard: String
    let meant: String
    let context: String
    let expect: Bool
    let note: String?
}

func loadCases(from path: String) throws -> [TestCase] {
    let data = try Data(contentsOf: URL(fileURLWithPath: path))
    return try JSONDecoder().decode(CaseFile.self, from: data).cases
}

/// Deterministic generator so batches are the same across runs and machines.
struct SeededGenerator: RandomNumberGenerator {
    private var state: UInt64
    init(seed: UInt64) { state = seed &+ 0x9E37_79B9_7F4A_7C15 }
    mutating func next() -> UInt64 {
        state &+= 0x9E37_79B9_7F4A_7C15
        var z = state
        z = (z ^ (z >> 30)) &* 0xBF58_476D_1CE4_E5B9
        z = (z ^ (z >> 27)) &* 0x94D0_49BB_1331_11EB
        return z ^ (z >> 31)
    }
}
