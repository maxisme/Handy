import Foundation
import FoundationModels

struct CallResult {
    /// One verdict per input pair, aligned to input order. nil when the call failed.
    let verdicts: [Bool]?
    let error: String?
    let milliseconds: Double
    /// Number of returned `meant` strings that did not match the input pair at that position.
    let meantMismatches: Int
}

struct Runner {
    let instructions: String
    let greedy: Bool
    let schema: Schema

    func availabilityProblem() -> String? {
        switch SystemLanguageModel.default.availability {
        case .available:
            return nil
        case .unavailable(let reason):
            return "Apple Intelligence unavailable: \(reason)"
        }
    }

    func prewarm() async {
        let session = LanguageModelSession(model: .default, instructions: instructions)
        session.prewarm()
        try? await Task.sleep(for: .milliseconds(500))
    }

    func call(pairs: [TestCase], withContext: Bool) async -> CallResult {
        // A fresh session per call so no transcript carries over between batches.
        let session = LanguageModelSession(model: .default, instructions: instructions)
        let prompt = buildUserPrompt(pairs: pairs, withContext: withContext)
        var options = GenerationOptions()
        if greedy {
            options = GenerationOptions(sampling: .greedy)
        }

        let clock = ContinuousClock()
        let start = clock.now
        do {
            let returned: [(meant: String, isVocabulary: Bool)]
            switch schema {
            case .bool:
                let response = try await session.respond(to: prompt, generating: VerdictList.self, options: options)
                returned = response.content.verdicts.map { ($0.meant, $0.isVocabulary) }
            case .kind:
                let response = try await session.respond(to: prompt, generating: KindVerdictList.self, options: options)
                returned = response.content.verdicts.map { ($0.meant, $0.kind.isVocabulary) }
            }
            let ms = elapsedMilliseconds(since: start, clock: clock)
            guard returned.count == pairs.count else {
                return CallResult(
                    verdicts: nil,
                    error: "expected \(pairs.count) verdicts, got \(returned.count)",
                    milliseconds: ms,
                    meantMismatches: 0
                )
            }
            var mismatches = 0
            for (index, verdict) in returned.enumerated() {
                let expected = pairs[index].meant.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
                let got = verdict.meant.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
                if expected != got { mismatches += 1 }
            }
            return CallResult(
                verdicts: returned.map { $0.isVocabulary },
                error: nil,
                milliseconds: ms,
                meantMismatches: mismatches
            )
        } catch {
            let ms = elapsedMilliseconds(since: start, clock: clock)
            return CallResult(verdicts: nil, error: "\(error)", milliseconds: ms, meantMismatches: 0)
        }
    }

    private func elapsedMilliseconds(since start: ContinuousClock.Instant, clock: ContinuousClock) -> Double {
        let duration = clock.now - start
        let parts = duration.components
        return Double(parts.seconds) * 1000 + Double(parts.attoseconds) / 1e15
    }
}
