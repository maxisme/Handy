//! The vocabulary check: the one judgment rules are bad at, made by the
//! post-processing model the user already set up. Apple Intelligence answers
//! through guided generation; any other provider answers through a JSON
//! schema. Both return a kind per pair, and yes or no is derived from the
//! kind in code, because asking the model for a boolean directly made it say
//! yes to almost every rewording.

use std::future::Future;

use log::debug;
use serde::{Deserialize, Serialize};

use super::prefilter::Candidate;
use crate::settings::{AppSettings, PostProcessProvider, APPLE_INTELLIGENCE_PROVIDER_ID};

/// What the model says the corrected text is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CorrectionKind {
    PersonName,
    ProductOrCompany,
    ProjectOrService,
    Acronym,
    TechnicalTerm,
    CommonWord,
    Rewording,
    Grammar,
    Formatting,
}

impl CorrectionKind {
    pub fn is_vocabulary(self) -> bool {
        matches!(
            self,
            Self::PersonName
                | Self::ProductOrCompany
                | Self::ProjectOrService
                | Self::Acronym
                | Self::TechnicalTerm
        )
    }
}

/// Instructions given to the model. Measured on Apple Intelligence at 1.00
/// precision and 0.88 recall over 62 labelled pairs with greedy decoding.
pub const INSTRUCTIONS: &str = "You classify corrections a person made to text written by a speech-to-text model. Each numbered pair gives what the model wrote (\"heard\"), what the person changed it to (\"meant\"), and the sentence it appeared in.

Look at \"meant\" and decide what kind of thing it is:
- personName: a person's name.
- productOrCompany: a product, brand, company or tool name, including compound names such as MacBook, GitHub or ChatGPT.
- projectOrService: an internal project, service, environment or codename.
- acronym: an acronym or initialism such as SDK, GDPR, R&D, SQL or CI/CD.
- technicalTerm: a specialist term from software, engineering, science, medicine, law or finance that a general dictionary would mark as jargon, such as webhook, monorepo, mutex, backfill, idempotent or cache.
- commonWord: an ordinary English word or phrase that a general dictionary lists as everyday language, even when \"heard\" was nonsense and even when the sentence is technical. Examples: source, from, there, obtain, believe, huge, assist.
- rewording: \"meant\" says the same thing as \"heard\" in different words.
- grammar: a change of tense, number, agreement, or the spelling of a common word.
- formatting: numbers, currency, percentages or punctuation.

The test for commonWord against technicalTerm: would a non-technical adult recognise \"meant\" as an everyday word? If yes, it is commonWord.

Return exactly one verdict per pair, in the same order, copying \"meant\" exactly as given.";

/// The numbered pairs the model is asked about, each with the edited sentence
/// for context.
pub fn user_prompt(candidates: &[Candidate], context: &str) -> String {
    let mut text = String::from("Pairs:\n");
    for (index, c) in candidates.iter().enumerate() {
        text.push_str(&format!(
            "{}. heard: \"{}\" — meant: \"{}\"\n   sentence: \"{}\"\n",
            index + 1,
            c.heard,
            c.meant,
            context
        ));
    }
    text
}

/// One verdict per candidate, in candidate order.
pub trait VocabularyCheck {
    fn check(
        &self,
        candidates: &[Candidate],
        context: &str,
    ) -> impl Future<Output = Result<Vec<CorrectionKind>, String>> + Send;
}

/// Why learning cannot run right now. `Ready` means it can.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Availability {
    Ready { provider_id: String, local: bool },
    PostProcessingOff,
    NoModel { provider_id: String },
    ProviderUnsupported { provider_id: String },
    AppleIntelligenceUnavailable,
}

/// Decide whether learning can run with the current settings, without
/// contacting any model.
pub fn availability(settings: &AppSettings) -> Availability {
    if !settings.post_process_enabled {
        return Availability::PostProcessingOff;
    }
    let Some(provider) = settings.active_post_process_provider() else {
        return Availability::PostProcessingOff;
    };
    let model = settings
        .post_process_models
        .get(&provider.id)
        .map(|m| m.trim().to_string())
        .unwrap_or_default();
    if model.is_empty() {
        return Availability::NoModel {
            provider_id: provider.id.clone(),
        };
    }
    if provider.id == APPLE_INTELLIGENCE_PROVIDER_ID {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            if !crate::apple_intelligence::check_apple_intelligence_availability() {
                return Availability::AppleIntelligenceUnavailable;
            }
            return Availability::Ready {
                provider_id: provider.id.clone(),
                local: true,
            };
        }
        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        {
            return Availability::AppleIntelligenceUnavailable;
        }
    }
    if !provider.supports_structured_output {
        return Availability::ProviderUnsupported {
            provider_id: provider.id.clone(),
        };
    }
    Availability::Ready {
        provider_id: provider.id.clone(),
        local: provider.id == "custom",
    }
}

/// The checker for the current settings, or the reason there is none.
pub fn checker(settings: &AppSettings) -> Result<Checker, Availability> {
    match availability(settings) {
        Availability::Ready { provider_id, .. }
            if provider_id == APPLE_INTELLIGENCE_PROVIDER_ID =>
        {
            Ok(Checker::AppleIntelligence)
        }
        Availability::Ready { provider_id, .. } => {
            let provider = settings
                .post_process_providers
                .iter()
                .find(|p| p.id == provider_id)
                .cloned()
                .ok_or(Availability::PostProcessingOff)?;
            let model = settings
                .post_process_models
                .get(&provider_id)
                .cloned()
                .unwrap_or_default();
            let api_key = settings
                .post_process_api_keys
                .get(&provider_id)
                .cloned()
                .unwrap_or_default();
            Ok(Checker::Provider(ProviderCheck {
                provider,
                model,
                api_key,
            }))
        }
        other => Err(other),
    }
}

/// A configured model that can run the check.
pub enum Checker {
    AppleIntelligence,
    Provider(ProviderCheck),
}

impl VocabularyCheck for Checker {
    async fn check(
        &self,
        candidates: &[Candidate],
        context: &str,
    ) -> Result<Vec<CorrectionKind>, String> {
        match self {
            Checker::AppleIntelligence => apple_check(candidates, context).await,
            Checker::Provider(p) => p.check(candidates, context).await,
        }
    }
}

#[derive(Debug, Deserialize)]
struct VerdictList {
    verdicts: Vec<Verdict>,
}

#[derive(Debug, Deserialize)]
struct Verdict {
    #[allow(dead_code)]
    meant: String,
    kind: CorrectionKind,
}

fn align(kinds: Vec<CorrectionKind>, expected: usize) -> Result<Vec<CorrectionKind>, String> {
    if kinds.len() != expected {
        return Err(format!(
            "model returned {} verdicts for {} pairs",
            kinds.len(),
            expected
        ));
    }
    Ok(kinds)
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
async fn apple_check(
    candidates: &[Candidate],
    context: &str,
) -> Result<Vec<CorrectionKind>, String> {
    let prompt = user_prompt(candidates, context);
    let expected = candidates.len();
    let verdicts = tokio::task::spawn_blocking(move || {
        crate::apple_intelligence::check_vocabulary(INSTRUCTIONS, &prompt)
    })
    .await
    .map_err(|e| format!("vocabulary check task failed: {e}"))??;
    let mismatched = verdicts
        .iter()
        .zip(candidates)
        .filter(|(v, c)| v.meant.trim() != c.meant.trim())
        .count();
    if mismatched > 0 {
        debug!("Apple Intelligence vocabulary check echoed {mismatched} pair(s) inexactly");
    }
    let kinds = verdicts
        .into_iter()
        .map(|v| {
            serde_json::from_value::<CorrectionKind>(serde_json::Value::String(v.kind.clone()))
                .map_err(|_| format!("unknown correction kind '{}'", v.kind))
        })
        .collect::<Result<Vec<_>, _>>()?;
    debug!(
        "Apple Intelligence vocabulary check returned {} verdicts",
        kinds.len()
    );
    align(kinds, expected)
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
async fn apple_check(
    _candidates: &[Candidate],
    _context: &str,
) -> Result<Vec<CorrectionKind>, String> {
    Err("Apple Intelligence is not available on this platform".to_string())
}

/// Any OpenAI-compatible provider that supports JSON schema output.
pub struct ProviderCheck {
    pub provider: PostProcessProvider,
    pub model: String,
    pub api_key: String,
}

impl ProviderCheck {
    fn schema() -> serde_json::Value {
        serde_json::json!({
            "name": "vocabulary_verdicts",
            "strict": true,
            "schema": {
                "type": "object",
                "properties": {
                    "verdicts": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "meant": { "type": "string" },
                                "kind": {
                                    "type": "string",
                                    "enum": [
                                        "personName", "productOrCompany", "projectOrService",
                                        "acronym", "technicalTerm", "commonWord", "rewording",
                                        "grammar", "formatting"
                                    ]
                                }
                            },
                            "required": ["meant", "kind"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["verdicts"],
                "additionalProperties": false
            }
        })
    }

    async fn check(
        &self,
        candidates: &[Candidate],
        context: &str,
    ) -> Result<Vec<CorrectionKind>, String> {
        let prompt = user_prompt(candidates, context);
        let disable_reasoning = matches!(self.provider.id.as_str(), "custom" | "openrouter");
        let content = crate::llm_client::send_chat_completion_with_schema(
            &self.provider,
            self.api_key.clone(),
            &self.model,
            prompt,
            Some(INSTRUCTIONS.to_string()),
            Some(Self::schema()),
            disable_reasoning,
        )
        .await?
        .ok_or_else(|| "empty response".to_string())?;
        let list: VerdictList = serde_json::from_str(&content)
            .map_err(|e| format!("vocabulary verdicts did not parse: {e}"))?;
        align(
            list.verdicts.into_iter().map(|v| v.kind).collect(),
            candidates.len(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_name_like_kinds_are_vocabulary() {
        for k in [
            CorrectionKind::PersonName,
            CorrectionKind::ProductOrCompany,
            CorrectionKind::ProjectOrService,
            CorrectionKind::Acronym,
            CorrectionKind::TechnicalTerm,
        ] {
            assert!(k.is_vocabulary(), "{k:?}");
        }
        for k in [
            CorrectionKind::CommonWord,
            CorrectionKind::Rewording,
            CorrectionKind::Grammar,
            CorrectionKind::Formatting,
        ] {
            assert!(!k.is_vocabulary(), "{k:?}");
        }
    }

    #[test]
    fn kinds_round_trip_as_camel_case() {
        let k: CorrectionKind = serde_json::from_str("\"productOrCompany\"").unwrap();
        assert_eq!(k, CorrectionKind::ProductOrCompany);
        assert!(serde_json::from_str::<CorrectionKind>("\"ProductOrCompany\"").is_err());
    }

    #[test]
    fn availability_follows_post_processing_settings() {
        let mut settings = AppSettings::default();
        assert_eq!(availability(&settings), Availability::PostProcessingOff);
        settings.post_process_enabled = true;
        settings.post_process_provider_id = "openai".to_string();
        settings
            .post_process_models
            .insert("openai".to_string(), String::new());
        assert_eq!(
            availability(&settings),
            Availability::NoModel {
                provider_id: "openai".to_string()
            }
        );
        settings
            .post_process_models
            .insert("openai".to_string(), "gpt-4.1-mini".to_string());
        assert_eq!(
            availability(&settings),
            Availability::Ready {
                provider_id: "openai".to_string(),
                local: false
            }
        );
    }

    /// Talks to the on-device model. Run by hand on a Mac with Apple
    /// Intelligence: `cargo test --lib apple_intelligence_classifies -- --ignored --nocapture`.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[tokio::test]
    #[ignore]
    async fn apple_intelligence_classifies_real_pairs() {
        let candidates = vec![
            Candidate {
                heard: "Charge B".into(),
                meant: "ChargeBee".into(),
            },
            Candidate {
                heard: "went to".into(),
                meant: "walked to".into(),
            },
            Candidate {
                heard: "s d k".into(),
                meant: "SDK".into(),
            },
            Candidate {
                heard: "form".into(),
                meant: "from".into(),
            },
        ];
        let kinds = Checker::AppleIntelligence
            .check(
                &candidates,
                "We moved billing to ChargeBee and walked to the SDK from here.",
            )
            .await
            .expect("Apple Intelligence answered");
        println!("{kinds:?}");
        assert_eq!(kinds.len(), 4);
        assert!(kinds[0].is_vocabulary());
        assert!(!kinds[1].is_vocabulary());
        assert!(kinds[2].is_vocabulary());
        assert!(!kinds[3].is_vocabulary());
    }

    #[test]
    fn user_prompt_numbers_pairs_with_context() {
        let c = vec![Candidate {
            heard: "Charge B".into(),
            meant: "ChargeBee".into(),
        }];
        assert_eq!(
            user_prompt(&c, "We moved billing to ChargeBee."),
            "Pairs:\n1. heard: \"Charge B\" — meant: \"ChargeBee\"\n   sentence: \"We moved billing to ChargeBee.\"\n"
        );
    }
}
