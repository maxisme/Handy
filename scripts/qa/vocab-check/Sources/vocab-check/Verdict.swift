import Foundation
import FoundationModels

@Generable
struct PairVerdict {
    @Guide(description: "The corrected text, copied exactly as given in the pair")
    let meant: String

    @Guide(description: "true if the corrected text is a proper noun, a product, company or project name, a person's name, an acronym, or a technical term that the speech-to-text model misheard. false if the change is rewording, a synonym, grammar, tense, punctuation, a common word misheard as another common word, number or currency formatting, or a plural of a common word.")
    let isVocabulary: Bool
}

@Generable
struct VerdictList {
    @Guide(description: "Exactly one verdict per numbered pair, in the same order as the pairs")
    let verdicts: [PairVerdict]
}

@Generable
enum CorrectionKind: String {
    case personName
    case productOrCompany
    case projectOrService
    case acronym
    case technicalTerm
    case commonWord
    case rewording
    case grammar
    case formatting

    var isVocabulary: Bool {
        switch self {
        case .personName, .productOrCompany, .projectOrService, .acronym, .technicalTerm:
            return true
        case .commonWord, .rewording, .grammar, .formatting:
            return false
        }
    }
}

@Generable
struct PairKindVerdict {
    @Guide(description: "The corrected text, copied exactly as given in the pair")
    let meant: String

    @Guide(description: "The kind of thing the corrected text is, using the definitions in the instructions")
    let kind: CorrectionKind
}

@Generable
struct KindVerdictList {
    @Guide(description: "Exactly one verdict per numbered pair, in the same order as the pairs")
    let verdicts: [PairKindVerdict]
}

enum Schema: String {
    case bool
    case kind
}

func buildUserPrompt(pairs: [TestCase], withContext: Bool) -> String {
    var text = "Pairs:\n"
    for (index, pair) in pairs.enumerated() {
        text += "\(index + 1). heard: \"\(pair.heard)\" — meant: \"\(pair.meant)\"\n"
        if withContext {
            text += "   sentence: \"\(pair.context)\"\n"
        }
    }
    return text
}
