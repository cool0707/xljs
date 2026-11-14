//! A chat set up for xljs: the AI chats that keep instructions of their
//! own (Microsoft 365 Copilot's agents, Gemini's Gems, ChatGPT's GPTs and
//! projects, Claude's projects). When the chat pane starts on one of these
//! services without such an assistant, the page offers to make one; this
//! gives what goes into it (the documents of docs/ai, built into the exe).

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::{Result, err, t};

/// The AI chats that can keep instructions for xljs.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Service {
    /// Microsoft 365 Copilot: an agent (instructions and skills).
    M365,
    /// Gemini: a Gem (instructions and knowledge).
    Gemini,
    /// ChatGPT: a GPT or a project (instructions and knowledge).
    ChatGpt,
    /// Claude: a project (instructions and knowledge).
    Claude,
}

impl Service {
    pub fn id(self) -> &'static str {
        match self {
            Service::M365 => "m365",
            Service::Gemini => "gemini",
            Service::ChatGpt => "chatgpt",
            Service::Claude => "claude",
        }
    }

    pub fn parse(s: &str) -> Option<Service> {
        [Service::M365, Service::Gemini, Service::ChatGpt, Service::Claude].into_iter().find(|v| v.id() == s)
    }

    /// What the service calls an assistant of one's own ("an agent").
    pub fn what(self) -> String {
        t!(&format!("dev.agentWhat.{}", self.id()))
    }

    /// The page that makes one, when there is a fixed one.
    fn create_url(self) -> &'static str {
        match self {
            Service::M365 => "",
            Service::Gemini => "https://gemini.google.com/gems/create",
            Service::ChatGpt => "https://chatgpt.com/gpts/editor",
            Service::Claude => "https://claude.ai/projects/create",
        }
    }
}

/// Which service a chat's URL is on, and whether it is an assistant of
/// one's own (an agent, a Gem, a GPT or a project) rather than the plain
/// chat. None for other pages (and Copilot without Microsoft 365, which
/// has no such assistants).
pub fn classify(url: &str) -> Option<(Service, bool)> {
    let rest = url.trim().split_once("://").map(|(s, r)| (s.to_ascii_lowercase(), r))?;
    if rest.0 != "https" && rest.0 != "http" {
        return None;
    }
    let rest = rest.1.split('#').next().unwrap_or("");
    let (hostport, path_query) = match rest.find(['/', '?']) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let host = hostport.rsplit('@').next().unwrap_or("").split(':').next().unwrap_or("").to_ascii_lowercase();
    let (path, query) = path_query.split_once('?').unwrap_or((path_query, ""));
    let segs: Vec<String> = path.split('/').filter(|s| !s.is_empty()).map(|s| s.to_ascii_lowercase()).collect();
    let seg = |name: &str| segs.iter().any(|s| s == name);
    let param = |name: &str| query.split('&').any(|kv| kv.split('=').next().is_some_and(|k| k.eq_ignore_ascii_case(name)));
    let on = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
    if on("cloud.microsoft") || on("microsoft365.com") || on("office.com") {
        // An agent's chat: ?titleId=T_... (or a path under /agent(s)).
        return Some((Service::M365, param("titleId") || seg("agent") || seg("agents")));
    }
    if host == "gemini.google.com" {
        return Some((Service::Gemini, seg("gem")));
    }
    if host == "chatgpt.com" || host == "chat.openai.com" {
        // GPTs and projects are both under /g/.
        return Some((Service::ChatGpt, segs.first().is_some_and(|s| s == "g")));
    }
    if host == "claude.ai" {
        return Some((Service::Claude, seg("project")));
    }
    None
}

/// The text of one of the documents for AI chats (a path under docs/ai).
pub fn doc(path: &str) -> Option<&'static str> {
    crate::embed::find(&crate::embed::AI_FILES, path).map(|f| f.text())
}

/// The skills for Copilot's agents (docs/ai/copilot/skills), in the order
/// they are offered.
const SKILLS: &[&str] = &["xljs-reference", "xljs-app-design", "xljs-pdf", "xljs-sqlite", "xljs-network", "xljs-chart", "vba-port"];

/// The description in a skill's front matter.
fn skill_description(text: &str) -> String {
    let Some(rest) = text.strip_prefix("---") else { return String::new() };
    let head = rest.split("\n---").next().unwrap_or("");
    head.lines().find_map(|l| l.strip_prefix("description:")).map(|d| d.trim().to_string()).unwrap_or_default()
}

/// A file offered to attach: the document (`file`, under docs/ai) and the
/// name it is dragged out with (`name`, relative: a skill keeps its
/// folder).
fn item(file: &str, name: &str, description: String) -> Value {
    json!({"file": file, "name": name, "description": description, "size": doc(file).map(|t| t.chars().count()).unwrap_or(0)})
}

/// What to put into an assistant of the service: an example name and
/// description, the instructions, and the files (skills or knowledge),
/// with the steps.
pub fn kit(s: Service) -> Value {
    let skill = |n: &str| format!("copilot/skills/{n}/SKILL.md");
    let skill_items = |as_folder: bool| -> Vec<Value> {
        SKILLS
            .iter()
            .map(|n| {
                let file = skill(n);
                let name = if as_folder { format!("{n}/SKILL.md") } else { format!("{n}.md") };
                item(&file, &name, skill_description(doc(&file).unwrap_or("")))
            })
            .collect()
    };
    let (instructions, files_kind, files) = match s {
        // The instructions fit Copilot's limit of 8,000 characters; the
        // reference is split into skills (30,000 characters each).
        Service::M365 => ("copilot/instructions.md", "skills", skill_items(true)),
        // A GPT's instructions have the same limit: the short instructions,
        // and the skills they name as knowledge files of those names.
        Service::ChatGpt => ("copilot/instructions.md", "knowledge", skill_items(false)),
        Service::Gemini | Service::Claude => ("prompt.md", "knowledge", vec![item("reference.md", "reference.md", t!("dev.agentReference"))]),
    };
    json!({
        "service": s.id(),
        "what": s.what(),
        "name": t!("dev.agentNameExample"),
        "description": t!("dev.agentDescriptionExample"),
        "steps": t!(&format!("dev.agentSteps.{}", s.id())),
        "instructions": item(instructions, instructions.rsplit('/').next().unwrap_or(instructions), String::new()),
        "filesKind": files_kind,
        "files": files,
        "createUrl": s.create_url(),
    })
}

/// Whether name is a relative path of plain parts (where a document is
/// written to be dragged out).
fn plain_relative(name: &str) -> bool {
    !name.is_empty()
        && name.split('/').all(|p| !p.is_empty() && p != "." && p != ".." && !p.contains(['\\', ':', '*', '?', '"', '<', '>', '|']))
}

/// Writes a document (file, under docs/ai) as dir/name for a drag to the
/// chat, and gives its path.
pub fn write_for_drag(dir: &Path, file: &str, name: &str) -> Result<PathBuf> {
    let text = doc(file).ok_or_else(|| err!("dev.agentNoFile", file))?;
    if !plain_relative(name) {
        return Err(err!("dev.agentNoFile", name));
    }
    let out = dir.join(name);
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| crate::Error::path("mkdir", parent.to_string_lossy(), e))?;
    }
    std::fs::write(&out, text).map_err(|e| crate::Error::path("write", out.to_string_lossy(), e))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_urls() {
        for (url, want) in [
            ("https://m365.cloud.microsoft/chat", Some((Service::M365, false))),
            ("https://m365.cloud.microsoft/chat/?auth=2", Some((Service::M365, false))),
            ("https://m365.cloud.microsoft/chat/?titleId=T_0123&source=embedded-builder", Some((Service::M365, true))),
            ("https://www.microsoft365.com/chat/agents/abc", Some((Service::M365, true))),
            ("https://copilot.cloud.microsoft/", Some((Service::M365, false))),
            ("https://gemini.google.com/app", Some((Service::Gemini, false))),
            ("https://gemini.google.com/app/0a1b2c", Some((Service::Gemini, false))),
            ("https://gemini.google.com/gem/9f8e7d", Some((Service::Gemini, true))),
            ("https://gemini.google.com/u/1/gem/9f8e7d/0a1b", Some((Service::Gemini, true))),
            ("https://chatgpt.com/", Some((Service::ChatGpt, false))),
            ("https://chatgpt.com/c/123", Some((Service::ChatGpt, false))),
            ("https://chatgpt.com/g/g-abc-xljs", Some((Service::ChatGpt, true))),
            ("https://chatgpt.com/g/g-p-abc/project", Some((Service::ChatGpt, true))),
            ("https://chat.openai.com/g/g-abc", Some((Service::ChatGpt, true))),
            ("https://claude.ai/new", Some((Service::Claude, false))),
            ("https://claude.ai/project/0193", Some((Service::Claude, true))),
            ("HTTPS://Claude.AI/Project/0193", Some((Service::Claude, true))),
            ("https://copilot.microsoft.com/", None),
            ("https://example.com/gem/1", None),
            ("https://gemini.google.com.example.com/gem/1", None),
            ("about:blank", None),
            ("", None),
        ] {
            assert_eq!(classify(url), want, "{url}");
        }
    }

    #[test]
    fn kits_name_built_in_documents() {
        let _l = crate::testutil::lock_lang("ja");
        for s in [Service::M365, Service::Gemini, Service::ChatGpt, Service::Claude] {
            let k = kit(s);
            assert_eq!(k["service"], s.id());
            for key in ["what", "steps", "name", "description"] {
                assert!(!k[key].as_str().unwrap().starts_with("dev."), "{key}: {}", k[key]);
            }
            let ins = k["instructions"]["file"].as_str().unwrap();
            assert!(doc(ins).is_some(), "{ins}");
            let files = k["files"].as_array().unwrap();
            assert!(!files.is_empty());
            for f in files {
                assert!(doc(f["file"].as_str().unwrap()).is_some(), "{f}");
                assert!(plain_relative(f["name"].as_str().unwrap()), "{f}");
                assert!(!f["description"].as_str().unwrap().is_empty(), "{f}");
            }
            assert_eq!(Service::parse(s.id()), Some(s));
        }
        // Every skill is offered to Copilot.
        let k = kit(Service::M365);
        let offered: Vec<&str> = k["files"].as_array().unwrap().iter().map(|f| f["file"].as_str().unwrap()).collect();
        for f in &crate::embed::AI_FILES {
            if f.name.starts_with("copilot/skills/") {
                assert!(offered.contains(&f.name), "{}", f.name);
            }
        }
    }

    #[test]
    fn written_for_drag() {
        let d = crate::testutil::tempdir();
        let p = write_for_drag(&d, "copilot/skills/xljs-pdf/SKILL.md", "xljs-pdf/SKILL.md").unwrap();
        assert_eq!(p, d.join("xljs-pdf").join("SKILL.md"));
        assert_eq!(std::fs::read_to_string(&p).unwrap(), doc("copilot/skills/xljs-pdf/SKILL.md").unwrap());
        assert!(write_for_drag(&d, "nothing.md", "x.md").is_err());
        assert!(write_for_drag(&d, "prompt.md", "../x.md").is_err());
        assert!(write_for_drag(&d, "prompt.md", "C:/x.md").is_err());
    }
}
