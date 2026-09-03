import Dispatch
import Foundation
import FoundationModels

@available(macOS 26.0, *)
@Generable
private struct CleanedTranscript: Sendable {
    let cleanedText: String
}

@available(macOS 26.0, *)
@Generable
private enum CorrectionKind: String, Sendable {
    case personName
    case productOrCompany
    case projectOrService
    case acronym
    case technicalTerm
    case commonWord
    case rewording
    case grammar
    case formatting
}

@available(macOS 26.0, *)
@Generable
private struct PairKindVerdict: Sendable {
    @Guide(description: "The corrected text, copied exactly as given in the pair")
    let meant: String
    @Guide(description: "The kind of thing the corrected text is, using the definitions in the instructions")
    let kind: CorrectionKind
}

@available(macOS 26.0, *)
@Generable
private struct KindVerdictList: Sendable {
    @Guide(description: "Exactly one verdict per numbered pair, in the same order as the pairs")
    let verdicts: [PairKindVerdict]
}

// Codable mirror of PairKindVerdict so the verdicts cross the C boundary as
// JSON with stable field names; `kind` carries the enum's raw value.
private struct KindVerdictJSON: Encodable {
    let meant: String
    let kind: String
}

// MARK: - Swift implementation for Apple LLM integration
// This file is compiled via Cargo build script for Apple Silicon targets

private typealias ResponsePointer = UnsafeMutablePointer<AppleLLMResponse>

private func duplicateCString(_ text: String) -> UnsafeMutablePointer<CChar>? {
    return text.withCString { basePointer in
        guard let duplicated = strdup(basePointer) else {
            return nil
        }
        return duplicated
    }
}

private func truncatedText(_ text: String, limit: Int) -> String {
    guard limit > 0 else { return text }
    let words = text.split(
        maxSplits: .max,
        omittingEmptySubsequences: true,
        whereSeparator: { $0.isWhitespace || $0.isNewline }
    )
    if words.count <= limit {
        return text
    }
    return words.prefix(limit).joined(separator: " ")
}

@_cdecl("is_apple_intelligence_available")
public func isAppleIntelligenceAvailable() -> Int32 {
    guard #available(macOS 26.0, *) else {
        return 0
    }

    let model = SystemLanguageModel.default
    switch model.availability {
    case .available:
        return 1
    case .unavailable:
        return 0
    }
}

@_cdecl("process_text_with_system_prompt_apple")
public func processTextWithSystemPrompt(
    _ systemPrompt: UnsafePointer<CChar>,
    _ userContent: UnsafePointer<CChar>,
    maxTokens: Int32
) -> UnsafeMutablePointer<AppleLLMResponse> {
    let swiftSystemPrompt = String(cString: systemPrompt)
    let swiftUserContent = String(cString: userContent)
    let responsePtr = ResponsePointer.allocate(capacity: 1)
    responsePtr.initialize(to: AppleLLMResponse(response: nil, success: 0, error_message: nil))

    guard #available(macOS 26.0, *) else {
        responsePtr.pointee.error_message = duplicateCString(
            "Apple Intelligence requires macOS 26 or newer."
        )
        return responsePtr
    }

    let model = SystemLanguageModel.default
    guard model.availability == .available else {
        responsePtr.pointee.error_message = duplicateCString(
            "Apple Intelligence is not currently available on this device."
        )
        return responsePtr
    }

    let tokenLimit = max(0, Int(maxTokens))
    let semaphore = DispatchSemaphore(value: 0)

    // Thread-safe container to pass results from async task back to calling thread
    final class ResultBox: @unchecked Sendable {
        var response: String?
        var error: String?
    }
    let box = ResultBox()

    Task.detached(priority: .userInitiated) {
        defer { semaphore.signal() }
        do {
            let session = LanguageModelSession(
                model: model,
                instructions: swiftSystemPrompt
            )
            // Guided generation keeps the model editing rather than replying:
            // asked for plain text it answers dictations that look like
            // requests. Greedy sampling keeps the edits consistent between
            // runs; with the default sampling the same sentence is converted
            // on one run and left alone on the next. The caller puts the whole
            // prompt, transcript included, in the user message for the same
            // reason (see actions.rs).
            let options = GenerationOptions(sampling: .greedy)
            let structured = try await session.respond(
                to: swiftUserContent,
                generating: CleanedTranscript.self,
                options: options
            )
            var output = structured.content.cleanedText
                .trimmingCharacters(in: .whitespacesAndNewlines)

            if tokenLimit > 0 {
                output = truncatedText(output, limit: tokenLimit)
            }
            box.response = output
        } catch {
            box.error = error.localizedDescription
        }
    }

    semaphore.wait()

    // Write to responsePtr on the calling thread after task completes
    if let response = box.response {
        responsePtr.pointee.response = duplicateCString(response)
        responsePtr.pointee.success = 1
    } else {
        responsePtr.pointee.error_message = duplicateCString(box.error ?? "Unknown error")
    }

    return responsePtr
}

@_cdecl("check_vocabulary_apple")
public func checkVocabularyApple(
    _ instructions: UnsafePointer<CChar>,
    _ userContent: UnsafePointer<CChar>
) -> UnsafeMutablePointer<AppleLLMResponse> {
    let swiftInstructions = String(cString: instructions)
    let swiftUserContent = String(cString: userContent)
    let responsePtr = ResponsePointer.allocate(capacity: 1)
    responsePtr.initialize(to: AppleLLMResponse(response: nil, success: 0, error_message: nil))

    guard #available(macOS 26.0, *) else {
        responsePtr.pointee.error_message = duplicateCString(
            "Apple Intelligence requires macOS 26 or newer."
        )
        return responsePtr
    }

    let model = SystemLanguageModel.default
    guard model.availability == .available else {
        responsePtr.pointee.error_message = duplicateCString(
            "Apple Intelligence is not currently available on this device."
        )
        return responsePtr
    }

    let semaphore = DispatchSemaphore(value: 0)

    // Thread-safe container to pass results from async task back to calling thread
    final class ResultBox: @unchecked Sendable {
        var response: String?
        var error: String?
    }
    let box = ResultBox()

    Task.detached(priority: .userInitiated) {
        defer { semaphore.signal() }
        do {
            let session = LanguageModelSession(
                model: model,
                instructions: swiftInstructions
            )
            // Guided generation pins the output to one verdict per pair with a
            // kind drawn from CorrectionKind, so no free-text parsing is needed.
            // Greedy sampling is required: with the default sampling the same
            // pair is classified differently from one run to the next.
            let options = GenerationOptions(sampling: .greedy)
            let structured = try await session.respond(
                to: swiftUserContent,
                generating: KindVerdictList.self,
                options: options
            )
            let verdicts = structured.content.verdicts.map { verdict in
                KindVerdictJSON(meant: verdict.meant, kind: verdict.kind.rawValue)
            }
            let data = try JSONEncoder().encode(verdicts)
            guard let json = String(data: data, encoding: .utf8) else {
                throw CocoaError(.coderInvalidValue)
            }
            box.response = json
        } catch {
            box.error = error.localizedDescription
        }
    }

    semaphore.wait()

    // Write to responsePtr on the calling thread after task completes
    if let response = box.response {
        responsePtr.pointee.response = duplicateCString(response)
        responsePtr.pointee.success = 1
    } else {
        responsePtr.pointee.error_message = duplicateCString(box.error ?? "Unknown error")
    }

    return responsePtr
}

@_cdecl("free_apple_llm_response")
public func freeAppleLLMResponse(_ response: UnsafeMutablePointer<AppleLLMResponse>?) {
    guard let response = response else { return }

    if let responseStr = response.pointee.response {
        free(UnsafeMutablePointer(mutating: responseStr))
    }

    if let errorStr = response.pointee.error_message {
        free(UnsafeMutablePointer(mutating: errorStr))
    }

    response.deallocate()
}