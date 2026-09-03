//! Assigns each dictation to a usage category from the app it was dictated
//! into and, for browsers and terminals, from the focused window's title.
//!
//! Categories, and what lands in each:
//!
//! | Category           | Rule                                                                 |
//! |--------------------|----------------------------------------------------------------------|
//! | `AiPrompts`        | AI chat/agent apps (Claude, ChatGPT, Cursor, Windsurf, Perplexity…), a browser tab whose title names an AI assistant, or a terminal whose title names a coding agent (Claude Code, Codex, Aider…). |
//! | `WorkMessages`     | Team chat: Slack, Microsoft Teams, Google Chat, Zoom, Mattermost.    |
//! | `PersonalMessages` | Messages, WhatsApp, Telegram, Signal, Discord, Messenger, Viber, WeChat, LINE. |
//! | `Emails`           | Mail clients (Mail, Outlook, Superhuman, Spark, Mimestream, Thunderbird…) and webmail tabs (Gmail, Outlook, Proton, Fastmail…). |
//! | `Documents`        | Word processors and notes (Pages, Word, Notes, Notion, Obsidian, Craft, Bear, Google Docs/Sheets/Slides, Confluence…). |
//! | `Code`             | Editors and IDEs (VS Code, Xcode, JetBrains, Zed, Sublime), terminals without an agent in the title, GitHub/GitLab tabs. |
//! | `Other`            | Everything else, including browser tabs no rule recognises.          |
//!
//! Matching is case-insensitive. App rules match the app id (bundle id on
//! macOS, executable name on Windows) by substring, or the app's display name
//! exactly. Title rules match the window title by substring. Rules are tried
//! in order, so put the more specific rule first where needles overlap
//! (Notion Mail before Notion, Google Chat before Google Docs).

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UsageCategory {
    AiPrompts,
    WorkMessages,
    PersonalMessages,
    Emails,
    Documents,
    Code,
    Other,
}

impl UsageCategory {
    /// Every category, in the order the insights page lists them.
    pub const ALL: [UsageCategory; 7] = [
        UsageCategory::AiPrompts,
        UsageCategory::WorkMessages,
        UsageCategory::PersonalMessages,
        UsageCategory::Emails,
        UsageCategory::Documents,
        UsageCategory::Code,
        UsageCategory::Other,
    ];
}

/// What an app is, before the window title is consulted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppKind {
    /// The app alone decides the category.
    Fixed(UsageCategory),
    /// The category depends on the open tab; unknown titles are `Other`.
    Browser,
    /// The category depends on what runs inside; unknown titles are `Code`.
    Terminal,
}

enum AppMatch {
    /// Substring of the lowercased app id.
    Id(&'static str),
    /// The whole lowercased display name.
    Name(&'static str),
}

use AppKind::{Browser, Fixed, Terminal};
use AppMatch::{Id, Name};
use UsageCategory::{AiPrompts, Code, Documents, Emails, Other, PersonalMessages, WorkMessages};

const APP_RULES: &[(AppMatch, AppKind)] = &[
    // Browsers
    (Id("com.google.chrome"), Browser),
    (Id("chrome.exe"), Browser),
    (Id("com.apple.safari"), Browser),
    (Id("org.mozilla.firefox"), Browser),
    (Id("firefox.exe"), Browser),
    (Id("company.thebrowser.browser"), Browser),
    (Id("company.thebrowser.dia"), Browser),
    (Id("com.brave.browser"), Browser),
    (Id("brave.exe"), Browser),
    (Id("com.microsoft.edgemac"), Browser),
    (Id("msedge.exe"), Browser),
    (Id("com.vivaldi.vivaldi"), Browser),
    (Id("com.operasoftware.opera"), Browser),
    (Id("opera.exe"), Browser),
    (Id("com.kagi.kagimacos"), Browser),
    (Id("app.zen-browser.zen"), Browser),
    (Name("arc"), Browser),
    (Name("dia"), Browser),
    (Name("zen"), Browser),
    (Name("orion"), Browser),
    // Terminals
    (Id("com.apple.terminal"), Terminal),
    (Id("com.googlecode.iterm2"), Terminal),
    (Id("com.mitchellh.ghostty"), Terminal),
    (Id("dev.warp.warp"), Terminal),
    (Id("net.kovidgoyal.kitty"), Terminal),
    (Id("org.alacritty"), Terminal),
    (Id("io.alacritty"), Terminal),
    (Id("com.github.wez.wezterm"), Terminal),
    (Id("co.zeit.hyper"), Terminal),
    (Id("org.tabby"), Terminal),
    (Id("windowsterminal.exe"), Terminal),
    (Id("wt.exe"), Terminal),
    (Id("cmd.exe"), Terminal),
    (Id("powershell.exe"), Terminal),
    (Id("pwsh.exe"), Terminal),
    (Id("conhost.exe"), Terminal),
    (Id("alacritty.exe"), Terminal),
    (Id("wezterm-gui.exe"), Terminal),
    // AI chat and agent apps
    (Id("com.anthropic."), Fixed(AiPrompts)),
    (Id("claude"), Fixed(AiPrompts)),
    (Id("com.openai."), Fixed(AiPrompts)),
    (Id("chatgpt"), Fixed(AiPrompts)),
    (Id("com.todesktop.230313mzl4w4u92"), Fixed(AiPrompts)), // Cursor
    (Name("cursor"), Fixed(AiPrompts)),
    (Id("com.exafunction.windsurf"), Fixed(AiPrompts)),
    (Name("windsurf"), Fixed(AiPrompts)),
    (Id("perplexity"), Fixed(AiPrompts)),
    (Id("com.google.gemini"), Fixed(AiPrompts)),
    (Id("copilot"), Fixed(AiPrompts)),
    (Id("com.electron.ollama"), Fixed(AiPrompts)),
    (Id("ai.elementlabs.lmstudio"), Fixed(AiPrompts)),
    (Id("xyz.chatboxapp"), Fixed(AiPrompts)),
    (Id("com.mistral"), Fixed(AiPrompts)),
    (Id("deepseek"), Fixed(AiPrompts)),
    (Id("grok"), Fixed(AiPrompts)),
    // Work messages
    (Id("com.tinyspeck.slackmacgap"), Fixed(WorkMessages)),
    (Id("slack"), Fixed(WorkMessages)),
    (Id("com.microsoft.teams"), Fixed(WorkMessages)),
    (Id("ms-teams.exe"), Fixed(WorkMessages)),
    (Id("teams.exe"), Fixed(WorkMessages)),
    (Id("us.zoom.xos"), Fixed(WorkMessages)),
    (Id("zoom.exe"), Fixed(WorkMessages)),
    (Id("com.google.chat"), Fixed(WorkMessages)),
    (Id("mattermost"), Fixed(WorkMessages)),
    // Personal messages
    (Id("com.apple.mobilesms"), Fixed(PersonalMessages)),
    (Name("messages"), Fixed(PersonalMessages)),
    (Id("whatsapp"), Fixed(PersonalMessages)),
    (Id("telegram"), Fixed(PersonalMessages)),
    (Id("signal"), Fixed(PersonalMessages)),
    (Id("com.hnc.discord"), Fixed(PersonalMessages)),
    (Id("discord"), Fixed(PersonalMessages)),
    (Id("com.facebook.archon"), Fixed(PersonalMessages)),
    (Name("messenger"), Fixed(PersonalMessages)),
    (Id("viber"), Fixed(PersonalMessages)),
    (Id("com.tencent.xinwechat"), Fixed(PersonalMessages)),
    (Name("wechat"), Fixed(PersonalMessages)),
    (Id("jp.naver.line"), Fixed(PersonalMessages)),
    (Id("com.beeper"), Fixed(PersonalMessages)),
    // Email
    (Id("notion.mail"), Fixed(Emails)),
    (Id("com.apple.mail"), Fixed(Emails)),
    (Name("mail"), Fixed(Emails)),
    (Id("com.microsoft.outlook"), Fixed(Emails)),
    (Id("olk.exe"), Fixed(Emails)),
    (Id("outlook.exe"), Fixed(Emails)),
    (Id("superhuman"), Fixed(Emails)),
    (Id("com.readdle.smartemail"), Fixed(Emails)),
    (Name("spark"), Fixed(Emails)),
    (Id("mimestream"), Fixed(Emails)),
    (Id("airmail"), Fixed(Emails)),
    (Id("thunderbird"), Fixed(Emails)),
    (Id("mailmate"), Fixed(Emails)),
    (Id("canarymail"), Fixed(Emails)),
    (Id("postbox"), Fixed(Emails)),
    // Documents
    (Id("com.apple.iwork.pages"), Fixed(Documents)),
    (Id("com.apple.iwork.keynote"), Fixed(Documents)),
    (Id("com.apple.iwork.numbers"), Fixed(Documents)),
    (Id("com.microsoft.word"), Fixed(Documents)),
    (Id("winword.exe"), Fixed(Documents)),
    (Id("com.microsoft.powerpoint"), Fixed(Documents)),
    (Id("powerpnt.exe"), Fixed(Documents)),
    (Id("com.microsoft.excel"), Fixed(Documents)),
    (Id("excel.exe"), Fixed(Documents)),
    (Id("com.microsoft.onenote"), Fixed(Documents)),
    (Id("onenote.exe"), Fixed(Documents)),
    (Id("com.apple.notes"), Fixed(Documents)),
    (Name("notes"), Fixed(Documents)),
    (Id("com.apple.textedit"), Fixed(Documents)),
    (Id("notepad.exe"), Fixed(Documents)),
    (Id("notion.id"), Fixed(Documents)),
    (Name("notion"), Fixed(Documents)),
    (Id("md.obsidian"), Fixed(Documents)),
    (Id("obsidian"), Fixed(Documents)),
    (Id("com.lukilabs.lukiapp"), Fixed(Documents)), // Craft
    (Name("craft"), Fixed(Documents)),
    (Id("net.shinyfrog.bear"), Fixed(Documents)),
    (Name("bear"), Fixed(Documents)),
    (Id("pro.writer.mac"), Fixed(Documents)),
    (Name("ia writer"), Fixed(Documents)),
    (Id("com.ulyssesapp"), Fixed(Documents)),
    (Id("typora"), Fixed(Documents)),
    (Id("scrivener"), Fixed(Documents)),
    (Id("logseq"), Fixed(Documents)),
    (Id("evernote"), Fixed(Documents)),
    (Id("goodnotes"), Fixed(Documents)),
    (Id("com.agiletortoise.drafts"), Fixed(Documents)),
    // Code editors and IDEs
    (Id("com.microsoft.vscode"), Fixed(Code)),
    (Id("code.exe"), Fixed(Code)),
    (Id("vscodium"), Fixed(Code)),
    (Id("com.apple.dt.xcode"), Fixed(Code)),
    (Id("com.jetbrains"), Fixed(Code)),
    (Id("com.google.android.studio"), Fixed(Code)),
    (Id("com.sublimetext"), Fixed(Code)),
    (Id("dev.zed.zed"), Fixed(Code)),
    (Id("com.panic.nova"), Fixed(Code)),
    (Id("neovide"), Fixed(Code)),
    (Id("idea64.exe"), Fixed(Code)),
    (Id("pycharm64.exe"), Fixed(Code)),
    (Id("webstorm64.exe"), Fixed(Code)),
    (Id("goland64.exe"), Fixed(Code)),
    (Id("rider64.exe"), Fixed(Code)),
    (Id("clion64.exe"), Fixed(Code)),
    (Id("devenv.exe"), Fixed(Code)),
];

/// Window-title rules for browsers. First match wins.
const BROWSER_TITLE_RULES: &[(&str, UsageCategory)] = &[
    // AI assistants
    ("claude", AiPrompts),
    ("chatgpt", AiPrompts),
    ("gemini", AiPrompts),
    ("perplexity", AiPrompts),
    ("copilot", AiPrompts),
    ("grok", AiPrompts),
    ("deepseek", AiPrompts),
    ("mistral", AiPrompts),
    ("notebooklm", AiPrompts),
    ("character.ai", AiPrompts),
    ("huggingchat", AiPrompts),
    ("meta ai", AiPrompts),
    ("lovable", AiPrompts),
    ("bolt.new", AiPrompts),
    ("v0.dev", AiPrompts),
    // Work messages (before documents so Google Chat beats Google Docs)
    ("slack", WorkMessages),
    ("microsoft teams", WorkMessages),
    ("google chat", WorkMessages),
    ("zoom", WorkMessages),
    ("mattermost", WorkMessages),
    // Personal messages
    ("whatsapp", PersonalMessages),
    ("telegram", PersonalMessages),
    ("messenger", PersonalMessages),
    ("discord", PersonalMessages),
    ("instagram", PersonalMessages),
    // Email
    ("gmail", Emails),
    ("outlook", Emails),
    ("proton mail", Emails),
    ("fastmail", Emails),
    ("superhuman", Emails),
    ("yahoo mail", Emails),
    ("icloud mail", Emails),
    ("zoho mail", Emails),
    ("notion mail", Emails),
    ("hey.com", Emails),
    // Documents
    ("google docs", Documents),
    ("google sheets", Documents),
    ("google slides", Documents),
    ("google forms", Documents),
    ("notion", Documents),
    ("confluence", Documents),
    ("| coda", Documents),
    ("dropbox paper", Documents),
    ("onenote", Documents),
    ("microsoft word", Documents),
    ("quip", Documents),
    ("overleaf", Documents),
    ("hackmd", Documents),
    ("obsidian", Documents),
    ("substack", Documents),
    // Code
    ("github", Code),
    ("gitlab", Code),
    ("bitbucket", Code),
    ("stackblitz", Code),
    ("codesandbox", Code),
    ("codepen", Code),
    ("replit", Code),
    ("jupyter", Code),
    ("colab", Code),
];

/// Window-title rules for terminals: coding agents that read typed prompts.
const TERMINAL_TITLE_RULES: &[(&str, UsageCategory)] = &[
    ("claude", AiPrompts),
    ("codex", AiPrompts),
    ("aider", AiPrompts),
    ("gemini", AiPrompts),
    ("copilot", AiPrompts),
    ("opencode", AiPrompts),
    ("goose", AiPrompts),
];

/// Category for one dictation. Any of the inputs may be missing; with nothing
/// known the answer is `Other`.
pub fn classify(
    app_id: Option<&str>,
    app_name: Option<&str>,
    window_title: Option<&str>,
) -> UsageCategory {
    let id = app_id.map(str::to_lowercase).unwrap_or_default();
    let name = app_name
        .map(|n| n.trim().to_lowercase())
        .unwrap_or_default();
    let title = window_title.map(str::to_lowercase).unwrap_or_default();

    let kind = APP_RULES.iter().find_map(|(needle, kind)| {
        let hit = match needle {
            Id(sub) => !id.is_empty() && id.contains(sub),
            Name(exact) => !name.is_empty() && name == *exact,
        };
        hit.then_some(*kind)
    });

    match kind {
        Some(Fixed(category)) => category,
        Some(Browser) => first_title_match(&title, BROWSER_TITLE_RULES).unwrap_or(Other),
        Some(Terminal) => first_title_match(&title, TERMINAL_TITLE_RULES).unwrap_or(Code),
        // Unknown apps still get a chance through their window title, which
        // covers web apps wrapped in unlisted browsers.
        None => first_title_match(&title, BROWSER_TITLE_RULES).unwrap_or(Other),
    }
}

fn first_title_match(title: &str, rules: &[(&str, UsageCategory)]) -> Option<UsageCategory> {
    if title.is_empty() {
        return None;
    }
    rules
        .iter()
        .find(|(needle, _)| title.contains(needle))
        .map(|(_, category)| *category)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_known_is_other() {
        assert_eq!(classify(None, None, None), Other);
        assert_eq!(classify(Some(""), Some(""), Some("")), Other);
    }

    #[test]
    fn native_apps_classify_by_bundle_id() {
        assert_eq!(
            classify(Some("com.tinyspeck.slackmacgap"), Some("Slack"), None),
            WorkMessages
        );
        assert_eq!(
            classify(Some("com.apple.MobileSMS"), Some("Messages"), Some("Alice")),
            PersonalMessages
        );
        assert_eq!(
            classify(Some("com.apple.mail"), Some("Mail"), Some("Inbox")),
            Emails
        );
        assert_eq!(
            classify(Some("notion.id"), Some("Notion"), Some("Roadmap")),
            Documents
        );
        assert_eq!(
            classify(Some("com.microsoft.VSCode"), Some("Code"), Some("main.rs")),
            Code
        );
        assert_eq!(
            classify(
                Some("com.anthropic.claudefordesktop"),
                Some("Claude"),
                Some("Claude")
            ),
            AiPrompts
        );
    }

    #[test]
    fn windows_apps_classify_by_executable_name() {
        assert_eq!(
            classify(Some("slack.exe"), Some("Slack"), None),
            WorkMessages
        );
        assert_eq!(classify(Some("olk.exe"), Some("olk"), None), Emails);
        assert_eq!(
            classify(Some("winword.exe"), Some("WINWORD"), None),
            Documents
        );
        assert_eq!(
            classify(
                Some("chrome.exe"),
                Some("chrome"),
                Some("Inbox - Gmail - Google Chrome")
            ),
            Emails
        );
    }

    #[test]
    fn display_name_matches_are_exact() {
        assert_eq!(classify(None, Some("Mail"), None), Emails);
        assert_eq!(classify(None, Some("Mailtrack Helper"), None), Other);
        assert_eq!(classify(None, Some("Arc"), Some("Claude")), AiPrompts);
    }

    #[test]
    fn browsers_classify_by_tab_title() {
        let chrome = Some("com.google.Chrome");
        assert_eq!(
            classify(
                chrome,
                Some("Google Chrome"),
                Some("Claude - Google Chrome")
            ),
            AiPrompts
        );
        assert_eq!(
            classify(chrome, Some("Google Chrome"), Some("ChatGPT")),
            AiPrompts
        );
        assert_eq!(
            classify(
                chrome,
                Some("Google Chrome"),
                Some("Inbox (3) - max@example.com - Gmail")
            ),
            Emails
        );
        assert_eq!(
            classify(chrome, Some("Google Chrome"), Some("Q3 plan - Google Docs")),
            Documents
        );
        assert_eq!(
            classify(
                chrome,
                Some("Google Chrome"),
                Some("general - Acme - Slack")
            ),
            WorkMessages
        );
        assert_eq!(
            classify(chrome, Some("Google Chrome"), Some("WhatsApp")),
            PersonalMessages
        );
        assert_eq!(
            classify(
                chrome,
                Some("Google Chrome"),
                Some("Pull Request #12 · acme/app - GitHub")
            ),
            Code
        );
        assert_eq!(
            classify(chrome, Some("Google Chrome"), Some("BBC News")),
            Other
        );
        assert_eq!(classify(chrome, Some("Google Chrome"), None), Other);
    }

    #[test]
    fn google_chat_beats_google_docs_in_title_rules() {
        assert_eq!(
            classify(
                Some("com.apple.Safari"),
                Some("Safari"),
                Some("Google Chat")
            ),
            WorkMessages
        );
    }

    #[test]
    fn terminals_are_code_unless_an_agent_is_in_the_title() {
        let ghostty = Some("com.mitchellh.ghostty");
        assert_eq!(classify(ghostty, Some("Ghostty"), Some("zsh")), Code);
        assert_eq!(classify(ghostty, Some("Ghostty"), None), Code);
        assert_eq!(
            classify(ghostty, Some("Ghostty"), Some("✳ Claude Code — handy")),
            AiPrompts
        );
        assert_eq!(
            classify(
                Some("com.apple.Terminal"),
                Some("Terminal"),
                Some("codex — 80×24")
            ),
            AiPrompts
        );
    }

    #[test]
    fn unknown_apps_fall_back_to_title_rules() {
        assert_eq!(
            classify(Some("com.example.kiosk"), Some("Kiosk"), Some("Gmail")),
            Emails
        );
        assert_eq!(
            classify(Some("com.example.kiosk"), Some("Kiosk"), Some("Settings")),
            Other
        );
    }

    #[test]
    fn every_category_is_listed_once() {
        let mut seen = std::collections::HashSet::new();
        for category in UsageCategory::ALL {
            assert!(seen.insert(category));
        }
    }
}
