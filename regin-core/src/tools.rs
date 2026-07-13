//! Tool definitions and execution for the regin agent.
//!
//! Tools: bash, read_file, write_file, edit_file, web_search, glob, grep
//! (FEAT-077 / DISC-021: `glob`/`grep` are dedicated, `.gitignore`-aware code
//! search tools backed by the `ignore`/`globset`/`regex` crates — the same
//! crate family ripgrep itself is built from — so the agent no longer has to
//! shell out to `find`/`grep` through `bash`.)

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;
use tracing::{debug, info};

/// Tool definition in OpenAI function-calling format.
#[derive(Debug, Clone, Serialize)]
pub struct ToolDef {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDef,
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// A tool call from the LLM response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// Result of executing a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub name: String,
    pub output: String,
    pub success: bool,
}

/// Tool definitions filtered to a persona's capability ceiling (FEAT-011), so the
/// LLM is only offered tools it is allowed to call. `None` → all tools.
pub fn tool_definitions_for(persona: Option<&crate::persona::Persona>) -> Vec<ToolDef> {
    tool_definitions()
        .into_iter()
        .filter(|d| crate::persona::allows(persona, &d.function.name))
        .collect()
}

/// Return all tool definitions for the LLM.
pub fn tool_definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            tool_type: "function".into(),
            function: FunctionDef {
                name: "bash".into(),
                description: "Execute a shell command via bash -c. Returns combined stdout and stderr.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command to execute"
                        },
                        "cwd": {
                            "type": "string",
                            "description": "Working directory (optional, defaults to caller cwd)"
                        }
                    },
                    "required": ["command"]
                }),
            },
        },
        ToolDef {
            tool_type: "function".into(),
            function: FunctionDef {
                name: "read_file".into(),
                description: "Read the full contents of a file.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute or relative path to the file"
                        }
                    },
                    "required": ["path"]
                }),
            },
        },
        ToolDef {
            tool_type: "function".into(),
            function: FunctionDef {
                name: "write_file".into(),
                description: "Write content to a file. Creates parent directories. Overwrites if exists.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file"
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to write"
                        }
                    },
                    "required": ["path", "content"]
                }),
            },
        },
        ToolDef {
            tool_type: "function".into(),
            function: FunctionDef {
                name: "edit_file".into(),
                description: "Replace a unique string in a file with new text.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file"
                        },
                        "old_text": {
                            "type": "string",
                            "description": "Exact text to find (must appear exactly once)"
                        },
                        "new_text": {
                            "type": "string",
                            "description": "Replacement text"
                        }
                    },
                    "required": ["path", "old_text", "new_text"]
                }),
            },
        },
        ToolDef {
            tool_type: "function".into(),
            function: FunctionDef {
                name: "web_search".into(),
                description: "Search the web via DuckDuckGo. Returns titles, URLs, and snippets.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query"
                        }
                    },
                    "required": ["query"]
                }),
            },
        },
        ToolDef {
            tool_type: "function".into(),
            function: FunctionDef {
                name: "glob".into(),
                description: "Find files by name pattern (e.g. \"**/*.rs\"). Respects .gitignore. Returns matching paths, most recently modified first.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Glob pattern to match file paths against (e.g. \"src/**/*.rs\", \"*.md\")"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory to search under (optional, defaults to caller cwd)"
                        }
                    },
                    "required": ["pattern"]
                }),
            },
        },
        ToolDef {
            tool_type: "function".into(),
            function: FunctionDef {
                name: "grep".into(),
                description: "Search file contents with a regex. Respects .gitignore. Returns file:line and one line of context on each side of every match.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Regular expression to search for"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory to search under (optional, defaults to caller cwd)"
                        },
                        "include": {
                            "type": "string",
                            "description": "Glob filter for which files to search (optional, e.g. \"*.rs\")"
                        }
                    },
                    "required": ["pattern"]
                }),
            },
        },
    ]
}

/// Execute a tool call, enforcing the guardrail (FEAT-038): the static global
/// red-lines and the editable per-role capability ceiling (FEAT-011). A denial is
/// refused before the tool runs, with an audit message naming the deciding layer.
pub async fn execute_tool_gated(
    call: &ToolCall,
    default_cwd: Option<&str>,
    persona: Option<&crate::persona::Persona>,
) -> ToolResult {
    let decision = crate::guardrail::check_tool_call(call, persona);
    if let Some(audit) = decision.audit() {
        tracing::warn!(tool = %call.function.name, "guardrail refused: {audit}");
        return ToolResult {
            tool_call_id: call.id.clone(),
            name: call.function.name.clone(),
            output: format!("Refused: {audit}"),
            success: false,
        };
    }
    execute_tool(call, default_cwd).await
}

/// Execute a tool call and return the result.
pub async fn execute_tool(call: &ToolCall, default_cwd: Option<&str>) -> ToolResult {
    let args: Value = serde_json::from_str(&call.function.arguments).unwrap_or(json!({}));
    info!(tool = %call.function.name, "Executing tool");

    let (output, success) = match call.function.name.as_str() {
        "bash" => exec_bash(&args, default_cwd),
        "read_file" => exec_read_file(&args),
        "write_file" => exec_write_file(&args),
        "edit_file" => exec_edit_file(&args),
        "web_search" => exec_web_search(&args).await,
        "glob" => exec_glob(&args, default_cwd),
        "grep" => exec_grep(&args, default_cwd),
        other => (format!("Unknown tool: {other}"), false),
    };

    debug!(tool = %call.function.name, success, output_len = output.len(), "Tool executed");

    ToolResult {
        tool_call_id: call.id.clone(),
        name: call.function.name.clone(),
        output,
        success,
    }
}

fn exec_bash(args: &Value, default_cwd: Option<&str>) -> (String, bool) {
    let command = args["command"].as_str().unwrap_or("");
    if command.is_empty() {
        return ("No command provided".into(), false);
    }
    let cwd = args["cwd"].as_str().or(default_cwd);

    let mut cmd = Command::new("bash");
    cmd.args(["-c", command]);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    match cmd.output() {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let mut result = String::new();
            if !stdout.is_empty() {
                result.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !result.is_empty() {
                    result.push_str("\n--- stderr ---\n");
                }
                result.push_str(&stderr);
            }
            if result.is_empty() {
                result = "(no output)".into();
            }
            (result, out.status.success())
        }
        Err(e) => (format!("Failed to execute: {e}"), false),
    }
}

fn exec_read_file(args: &Value) -> (String, bool) {
    let path = args["path"].as_str().unwrap_or("");
    if path.is_empty() {
        return ("No path provided".into(), false);
    }
    match std::fs::read_to_string(path) {
        Ok(c) => (c, true),
        Err(e) => (format!("Error reading {path}: {e}"), false),
    }
}

fn exec_write_file(args: &Value) -> (String, bool) {
    let path = args["path"].as_str().unwrap_or("");
    let content = args["content"].as_str().unwrap_or("");
    if path.is_empty() {
        return ("No path provided".into(), false);
    }
    let p = Path::new(path);
    if let Some(parent) = p.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return (format!("Failed to create directories: {e}"), false);
        }
    }
    match std::fs::write(path, content) {
        Ok(()) => (format!("Wrote {} bytes to {path}", content.len()), true),
        Err(e) => (format!("Error writing {path}: {e}"), false),
    }
}

fn exec_edit_file(args: &Value) -> (String, bool) {
    let path = args["path"].as_str().unwrap_or("");
    let old_text = args["old_text"].as_str().unwrap_or("");
    let new_text = args["new_text"].as_str().unwrap_or("");
    if path.is_empty() || old_text.is_empty() {
        return ("path and old_text are required".into(), false);
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => return (format!("Error reading {path}: {e}"), false),
    };
    let count = content.matches(old_text).count();
    if count == 0 {
        return (format!("old_text not found in {path}"), false);
    }
    if count > 1 {
        return (format!("old_text appears {count} times in {path} (must be unique)"), false);
    }
    let updated = content.replacen(old_text, new_text, 1);
    match std::fs::write(path, &updated) {
        Ok(()) => (format!("Edited {path}"), true),
        Err(e) => (format!("Error writing {path}: {e}"), false),
    }
}

/// Cap on grep matches returned in one call — a runaway pattern against a
/// huge tree shouldn't flood the agent's context.
const MAX_GREP_MATCHES: usize = 200;

/// A `.gitignore`-aware directory walker for `glob`/`grep`. `require_git(false)`
/// so `.gitignore` is honoured even outside an actual git repo (e.g. a working
/// copy before `git init`) — `ignore`'s default only applies `.gitignore` rules
/// inside a real repo, which is narrower than what a code-search tool wants.
fn code_search_walker(base: &Path) -> ignore::Walk {
    ignore::WalkBuilder::new(base).require_git(false).build()
}

fn exec_glob(args: &Value, default_cwd: Option<&str>) -> (String, bool) {
    let pattern = args["pattern"].as_str().unwrap_or("");
    if pattern.is_empty() {
        return ("No pattern provided".into(), false);
    }
    let base = args["path"].as_str().or(default_cwd).unwrap_or(".");
    let base_path = Path::new(base);
    if !base_path.exists() {
        return (format!("Path not found: {base}"), false);
    }
    let matcher = match globset::Glob::new(pattern) {
        Ok(g) => g.compile_matcher(),
        Err(e) => return (format!("Invalid glob pattern {pattern:?}: {e}"), false),
    };

    let mut matches: Vec<(std::path::PathBuf, std::time::SystemTime)> = Vec::new();
    for entry in code_search_walker(base_path) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let rel = entry.path().strip_prefix(base_path).unwrap_or(entry.path());
        if !matcher.is_match(rel) {
            continue;
        }
        let modified = entry.metadata().ok().and_then(|m| m.modified().ok()).unwrap_or(std::time::UNIX_EPOCH);
        matches.push((entry.into_path(), modified));
    }

    if matches.is_empty() {
        return ("No files matched".into(), true);
    }
    matches.sort_by(|a, b| b.1.cmp(&a.1));
    let output = matches.into_iter().map(|(p, _)| p.display().to_string()).collect::<Vec<_>>().join("\n");
    (output, true)
}

fn exec_grep(args: &Value, default_cwd: Option<&str>) -> (String, bool) {
    let pattern = args["pattern"].as_str().unwrap_or("");
    if pattern.is_empty() {
        return ("No pattern provided".into(), false);
    }
    let base = args["path"].as_str().or(default_cwd).unwrap_or(".");
    let base_path = Path::new(base);
    if !base_path.exists() {
        return (format!("Path not found: {base}"), false);
    }
    let re = match regex::Regex::new(pattern) {
        Ok(r) => r,
        Err(e) => return (format!("Invalid regex {pattern:?}: {e}"), false),
    };
    let include_matcher = match args["include"].as_str() {
        Some(inc) if !inc.is_empty() => match globset::Glob::new(inc) {
            Ok(g) => Some(g.compile_matcher()),
            Err(e) => return (format!("Invalid include pattern {inc:?}: {e}"), false),
        },
        _ => None,
    };

    let mut out = String::new();
    let mut hits = 0usize;
    'walk: for entry in code_search_walker(base_path) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        if let Some(m) = &include_matcher {
            let rel = entry.path().strip_prefix(base_path).unwrap_or(entry.path());
            if !m.is_match(rel) {
                continue;
            }
        }
        // A read error here is almost always a binary file — skip it rather
        // than surfacing noise for every non-UTF8 asset in the tree.
        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let lines: Vec<&str> = content.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if !re.is_match(line) {
                continue;
            }
            hits += 1;
            if hits > MAX_GREP_MATCHES {
                out += "... (truncated — more matches exist, narrow the pattern or path)\n";
                break 'walk;
            }
            out += &format!("{}:{}:\n", entry.path().display(), i + 1);
            if i > 0 {
                out += &format!("  {}\n", lines[i - 1]);
            }
            out += &format!("> {line}\n");
            if let Some(after) = lines.get(i + 1) {
                out += &format!("  {after}\n");
            }
        }
    }

    if hits == 0 {
        return ("No matches found".into(), true);
    }
    (out, true)
}

async fn exec_web_search(args: &Value) -> (String, bool) {
    let query = args["query"].as_str().unwrap_or("");
    if query.is_empty() {
        return ("No query provided".into(), false);
    }

    let url = format!("https://html.duckduckgo.com/html/?q={}", urlencoding(query));
    let client = match reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (compatible; Regin/0.2)")
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(e) => return (format!("HTTP client error: {e}"), false),
    };

    match client.get(&url).send().await {
        Ok(resp) => match resp.text().await {
            Ok(html) => {
                let results = parse_ddg_results(&html);
                if results.is_empty() {
                    ("No results found.".into(), true)
                } else {
                    (results.join("\n\n"), true)
                }
            }
            Err(e) => (format!("Failed to read response: {e}"), false),
        },
        Err(e) => (format!("Search request failed: {e}"), false),
    }
}

fn urlencoding(s: &str) -> String {
    let mut result = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            b' ' => result.push('+'),
            _ => result.push_str(&format!("%{:02X}", b)),
        }
    }
    result
}

fn parse_ddg_results(html: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut pos = 0;
    while let Some(start) = html[pos..].find("class=\"result__a\"") {
        let abs = pos + start;
        let href_start = html[..abs].rfind("href=\"").map(|i| i + 6);
        let href = href_start.and_then(|s| html[s..].find('"').map(|e| &html[s..s + e]));

        let title = html[abs..].find('>').and_then(|s| {
            let start = abs + s + 1;
            html[start..].find("</a>").map(|e| strip_tags(&html[start..start + e]))
        });

        let snippet = html[abs..].find("result__snippet").and_then(|s| {
            let sabs = abs + s;
            html[sabs..].find('>').and_then(|gt| {
                let start = sabs + gt + 1;
                html[start..].find('<').map(|e| strip_tags(&html[start..start + e]))
            })
        });

        if let (Some(title), Some(href)) = (title, href) {
            let mut entry = format!("**{}**\n{}", title.trim(), href);
            if let Some(snip) = snippet {
                let snip = snip.trim();
                if !snip.is_empty() {
                    entry.push_str(&format!("\n{snip}"));
                }
            }
            results.push(entry);
        }
        pos = abs + 10;
        if results.len() >= 10 {
            break;
        }
    }
    results
}

fn strip_tags(s: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
mod persona_gate_tests {
    use super::*;
    use crate::persona::Persona;

    fn call(name: &str) -> ToolCall {
        ToolCall {
            id: "c1".into(),
            call_type: "function".into(),
            function: FunctionCall { name: name.into(), arguments: "{}".into() },
        }
    }

    #[tokio::test]
    async fn gated_tool_outside_ceiling_is_refused() {
        let p = Persona::from_toml("role = \"reader\"\ntools = [\"read_file\"]\n").unwrap();
        let r = execute_tool_gated(&call("web_search"), None, Some(&p)).await;
        assert!(!r.success);
        assert!(r.output.contains("ceiling"), "got: {}", r.output);
    }

    #[test]
    fn filtered_definitions_match_the_ceiling() {
        let p = Persona::from_toml("role = \"reader\"\ntools = [\"read_file\", \"web_search\"]\n").unwrap();
        let names: Vec<String> = tool_definitions_for(Some(&p)).into_iter().map(|d| d.function.name).collect();
        assert!(names.contains(&"read_file".to_string()) && names.contains(&"web_search".to_string()));
        assert!(!names.contains(&"bash".to_string()), "bash filtered out");
        // unscoped sees everything
        assert_eq!(tool_definitions_for(None).len(), tool_definitions().len());
    }
}

#[cfg(test)]
mod exec_tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    fn tmp() -> PathBuf {
        let p = std::env::temp_dir().join(format!("regin-tools-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn call(name: &str, args: serde_json::Value) -> ToolCall {
        ToolCall {
            id: "t".into(),
            call_type: "function".into(),
            function: FunctionCall { name: name.into(), arguments: args.to_string() },
        }
    }

    #[tokio::test]
    async fn bash_runs_and_reports_empty_command() {
        let ok = execute_tool(&call("bash", json!({"command": "echo hello-regin"})), None).await;
        assert!(ok.success);
        assert!(ok.output.contains("hello-regin"));

        let empty = execute_tool(&call("bash", json!({"command": ""})), None).await;
        assert!(!empty.success);
        assert!(empty.output.contains("No command"));
    }

    #[tokio::test]
    async fn write_read_edit_roundtrip_and_missing_file() {
        let dir = tmp();
        let path = dir.join("note.txt");
        let p = path.to_str().unwrap();

        let w = execute_tool(&call("write_file", json!({"path": p, "content": "foo bar"})), None).await;
        assert!(w.success, "{}", w.output);

        let r = execute_tool(&call("read_file", json!({"path": p})), None).await;
        assert!(r.success);
        assert!(r.output.contains("foo bar"));

        let e = execute_tool(&call("edit_file", json!({"path": p, "old_text": "foo", "new_text": "baz"})), None).await;
        assert!(e.success, "{}", e.output);
        let r2 = execute_tool(&call("read_file", json!({"path": p})), None).await;
        assert!(r2.output.contains("baz bar"));

        let miss = execute_tool(&call("read_file", json!({"path": dir.join("nope").to_str().unwrap()})), None).await;
        assert!(!miss.success);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn glob_finds_matches_sorted_by_recency_and_respects_gitignore() {
        // acceptance criteria 1 and 6
        let dir = tmp();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/a.rs"), "fn a() {}").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(dir.join("src/b.rs"), "fn b() {}").unwrap();
        std::fs::write(dir.join("src/c.txt"), "not rust").unwrap();
        std::fs::write(dir.join(".gitignore"), "ignored.rs\n").unwrap();
        std::fs::write(dir.join("src/ignored.rs"), "fn ignored() {}").unwrap();

        let r = execute_tool(&call("glob", json!({"pattern": "**/*.rs", "path": dir.to_str().unwrap()})), None).await;
        assert!(r.success, "{}", r.output);
        assert!(r.output.contains("a.rs"));
        assert!(r.output.contains("b.rs"));
        assert!(!r.output.contains("c.txt"));
        assert!(!r.output.contains("ignored.rs"), "gitignore respected: {}", r.output);
        // most recently modified first
        let b_pos = r.output.find("b.rs").unwrap();
        let a_pos = r.output.find("a.rs").unwrap();
        assert!(b_pos < a_pos, "b.rs (newer) should sort before a.rs (older): {}", r.output);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn glob_reports_empty_and_invalid_pattern_and_missing_path() {
        let dir = tmp();
        let empty = execute_tool(&call("glob", json!({"pattern": "*.nope", "path": dir.to_str().unwrap()})), None).await;
        assert!(empty.success);
        assert!(empty.output.contains("No files matched"));

        let bad = execute_tool(&call("glob", json!({"pattern": "["})), None).await;
        assert!(!bad.success);
        assert!(bad.output.contains("Invalid glob pattern"));

        let missing = execute_tool(&call("glob", json!({"pattern": "*", "path": "/no/such/dir"})), None).await;
        assert!(!missing.success);
        assert!(missing.output.contains("Path not found"));

        let no_pattern = execute_tool(&call("glob", json!({})), None).await;
        assert!(!no_pattern.success);
        assert!(no_pattern.output.contains("No pattern"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn grep_finds_matches_with_context_and_respects_include_and_gitignore() {
        // acceptance criteria 2 and 6
        let dir = tmp();
        std::fs::write(dir.join("main.rs"), "line one\nfn target() {}\nline three\n").unwrap();
        std::fs::write(dir.join("notes.txt"), "target mentioned here too\n").unwrap();
        std::fs::write(dir.join(".gitignore"), "skip.rs\n").unwrap();
        std::fs::write(dir.join("skip.rs"), "fn target() {}\n").unwrap();

        let r = execute_tool(&call("grep", json!({"pattern": "target", "path": dir.to_str().unwrap(), "include": "*.rs"})), None).await;
        assert!(r.success, "{}", r.output);
        assert!(r.output.contains("main.rs:2:"));
        assert!(r.output.contains("line one"), "context before: {}", r.output);
        assert!(r.output.contains("line three"), "context after: {}", r.output);
        assert!(!r.output.contains("notes.txt"), "include filter respected");
        assert!(!r.output.contains("skip.rs"), "gitignore respected: {}", r.output);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn grep_reports_no_matches_and_invalid_regex_and_include() {
        let dir = tmp();
        std::fs::write(dir.join("f.txt"), "hello world\n").unwrap();

        let none = execute_tool(&call("grep", json!({"pattern": "nowhere", "path": dir.to_str().unwrap()})), None).await;
        assert!(none.success);
        assert!(none.output.contains("No matches found"));

        let bad_regex = execute_tool(&call("grep", json!({"pattern": "(unclosed"})), None).await;
        assert!(!bad_regex.success);
        assert!(bad_regex.output.contains("Invalid regex"));

        let bad_include = execute_tool(&call("grep", json!({"pattern": "hello", "path": dir.to_str().unwrap(), "include": "["})), None).await;
        assert!(!bad_include.success);
        assert!(bad_include.output.contains("Invalid include pattern"));

        let missing = execute_tool(&call("grep", json!({"pattern": "x", "path": "/no/such/dir"})), None).await;
        assert!(!missing.success);
        assert!(missing.output.contains("Path not found"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn grep_truncates_past_the_match_cap() {
        let dir = tmp();
        let mut content = String::new();
        for i in 0..(MAX_GREP_MATCHES + 20) {
            content += &format!("needle {i}\n");
        }
        std::fs::write(dir.join("big.txt"), content).unwrap();

        let r = execute_tool(&call("grep", json!({"pattern": "needle", "path": dir.to_str().unwrap()})), None).await;
        assert!(r.success);
        assert!(r.output.contains("truncated"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn unknown_tool_is_reported() {
        let r = execute_tool(&call("telepathy", json!({})), None).await;
        assert!(!r.success);
        assert!(r.output.contains("Unknown tool"));
    }

    #[test]
    fn urlencoding_escapes_reserved() {
        assert_eq!(urlencoding("a b/c?"), "a+b%2Fc%3F");
        assert_eq!(urlencoding("plain-Text_1.0~"), "plain-Text_1.0~");
    }

    #[test]
    fn strip_tags_and_ddg_parse() {
        assert_eq!(strip_tags("<b>hi</b> <i>there</i>"), "hi there");
        let html = r#"<a href="https://example.com" class="result__a">Example Title</a>"#;
        let results = parse_ddg_results(html);
        assert!(results.iter().any(|r| r.contains("Example Title") && r.contains("example.com")));
        assert!(parse_ddg_results("<html>no results</html>").is_empty());
    }
}
