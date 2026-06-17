//! Tool definitions and execution for the regin agent.
//!
//! Tools: bash, read_file, write_file, edit_file, web_search

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
    ]
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
