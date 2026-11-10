//! The skills this server hands an agent: how to use its tools well, written once and read
//! by whichever client connects - the MCP Skills extension, `io.modelcontextprotocol/skills`.
//!
//! A skill is a `SKILL.md` kept beside this file and built into the executable, so what a
//! client is told is always about the tools of the moon it is talking to. It is listed with
//! `skills/list`, described with `skills/get` and read, like any resource, with
//! `resources/read` at `skill://<name>/SKILL.md`.

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

/// What a server that has skills says so under, in its capabilities' `extensions`.
pub(super) const EXTENSION: &str = "io.modelcontextprotocol/skills";

/// How long a client may keep what it was told, in milliseconds: the skills are the
/// executable's, and change only when moon is updated.
const KEPT_FOR_MS: u64 = 300_000;

const SKILL_FILE_NAME: &str = "SKILL.md";
const SKILL_MIME_TYPE: &str = "text/markdown";

/// One skill: the name its folder has, which its frontmatter repeats, and its `SKILL.md`.
struct Skill {
    name: &'static str,
    text: &'static str,
}

const SKILLS: &[Skill] = &[Skill {
    name: "moontasks-interaction",
    text: include_str!("skills/moontasks-interaction/SKILL.md"),
}];

impl Skill {
    /// Where its `SKILL.md` is read: `skill://moontasks-interaction/SKILL.md`.
    fn uri(&self) -> String {
        format!("skill://{}/{SKILL_FILE_NAME}", self.name)
    }

    /// The fields between the `---` lines its file starts with, as they are written: a
    /// client reads the file's own and holds this to them, field for field.
    fn read_frontmatter(&self) -> Map<String, Value> {
        let mut fields = Map::new();
        let mut lines = self.text.lines();
        if lines.next() != Some("---") {
            return fields;
        }
        for line in lines.take_while(|line| *line != "---") {
            if let Some((field, value)) = line.split_once(':') {
                fields.insert(field.trim().to_string(), json!(value.trim()));
            }
        }
        fields
    }

    /// The skill as a listing says it: its file, its frontmatter, and every file it is made
    /// of with the size and the SHA-256 of its bytes.
    fn describe(&self) -> Value {
        let digest: String = Sha256::digest(self.text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        json!({
            "uri": self.uri(),
            "frontmatter": self.read_frontmatter(),
            "resources": [{
                "uri": self.uri(),
                "digest": format!("sha256:{digest}"),
                "size": self.text.len(),
            }],
        })
    }
}

fn find_skill_at(uri: &str) -> Option<&'static Skill> {
    SKILLS.iter().find(|skill| skill.uri() == uri)
}

/// A result a client may keep for a while, and share: nothing in a skill is anybody's own.
fn answer_complete(mut result: Value) -> Value {
    let fields = result.as_object_mut().expect("a result is an object");
    fields.insert("resultType".to_string(), json!("complete"));
    fields.insert("ttlMs".to_string(), json!(KEPT_FOR_MS));
    fields.insert("cacheScope".to_string(), json!("public"));
    result
}

/// The `uri` a request names, or what to say to one that names none.
fn read_uri_asked_for(params: Option<&Value>) -> Result<&str, String> {
    params
        .and_then(|params| params.get("uri"))
        .and_then(Value::as_str)
        .ok_or_else(|| "the request names no `uri`".to_string())
}

/// `skills/list`: every skill of the server.
pub(super) fn list_skills() -> Value {
    let skills: Vec<Value> = SKILLS.iter().map(Skill::describe).collect();
    answer_complete(json!({ "skills": skills }))
}

/// `skills/get`: one skill, by the address of its `SKILL.md`.
pub(super) fn describe_skill_at(params: Option<&Value>) -> Result<Value, String> {
    let uri = read_uri_asked_for(params)?;
    let skill = find_skill_at(uri).ok_or_else(|| format!("{uri} is no skill of this server"))?;
    Ok(answer_complete(json!({ "skill": skill.describe() })))
}

/// `resources/list`: the files there are to read, which are the skills' own.
pub(super) fn list_resources() -> Value {
    let resources: Vec<Value> = SKILLS
        .iter()
        .map(|skill| {
            let frontmatter = skill.read_frontmatter();
            json!({
                "uri": skill.uri(),
                "name": frontmatter.get("name").cloned().unwrap_or_else(|| json!(skill.name)),
                "description": frontmatter.get("description").cloned().unwrap_or_default(),
                "mimeType": SKILL_MIME_TYPE,
            })
        })
        .collect();
    answer_complete(json!({ "resources": resources }))
}

/// `resources/read`: the text of one of those files.
pub(super) fn read_resource_at(params: Option<&Value>) -> Result<Value, String> {
    let uri = read_uri_asked_for(params)?;
    let skill =
        find_skill_at(uri).ok_or_else(|| format!("{uri} is no resource of this server"))?;
    Ok(answer_complete(json!({
        "contents": [{ "uri": skill.uri(), "mimeType": SKILL_MIME_TYPE, "text": skill.text }],
    })))
}

/// The address of every skill, for the server's greeting to point a client at.
pub(super) fn list_skill_uris() -> Vec<String> {
    SKILLS.iter().map(Skill::uri).collect()
}
