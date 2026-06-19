//! Capability ceiling + global red-lines (FEAT-038 / DISC-009).
//!
//! Every tool call is checked against **two layers**:
//!
//! 1. **Global red-lines** — a static, compiled-in, *non-runtime-adjustable* set
//!    of prohibitions no role may ever cross (constitutional). They protect the
//!    safety substrate (backups, audit log, KPI store), governance (regin's own
//!    service, escalation, kill-switch) and the host from catastrophe.
//! 2. **Capability ceiling** — the editable per-role tool ceiling from the
//!    persona (FEAT-011), the day-to-day authorization floor (statutory).
//!
//! Red-lines are checked first and win, so a permissive ceiling can never grant a
//! red-line action. Denials name the deciding layer for the audit trail.
//!
//! Red-line matching on shell commands is necessarily heuristic — this is
//! defense-in-depth (regin ingests logs, a prompt-injection surface, and the
//! ceiling is editable), not a sandbox. It errs toward catching the unambiguous
//! catastrophes without blocking ordinary operator remediation.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::persona::Persona;
use crate::tools::ToolCall;

/// Which layer rendered a denial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecisionLayer {
    /// The static, non-runtime-adjustable global red-lines.
    RedLine,
    /// The editable per-role capability ceiling.
    Ceiling,
}

impl std::fmt::Display for DecisionLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecisionLayer::RedLine => write!(f, "red-line"),
            DecisionLayer::Ceiling => write!(f, "ceiling"),
        }
    }
}

/// The outcome of a guardrail check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    Allow,
    Deny { layer: DecisionLayer, reason: String },
}

impl Decision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Decision::Allow)
    }

    /// A clear "which layer denied this" audit message, when denied.
    pub fn audit(&self) -> Option<String> {
        match self {
            Decision::Allow => None,
            Decision::Deny { layer, reason } => Some(format!("[{layer}] {reason}")),
        }
    }
}

/// Check a tool call against the global red-lines (static) then the editable
/// capability ceiling (persona). Red-lines take precedence.
pub fn check_tool_call(call: &ToolCall, persona: Option<&Persona>) -> Decision {
    let name = call.function.name.as_str();
    let args: Value = serde_json::from_str(&call.function.arguments).unwrap_or(Value::Null);

    if let Some(reason) = red_line_violation(name, &args) {
        return Decision::Deny { layer: DecisionLayer::RedLine, reason };
    }
    if !crate::persona::allows(persona, name) {
        let role = persona.map(|p| p.role.as_str()).unwrap_or("?");
        return Decision::Deny {
            layer: DecisionLayer::Ceiling,
            reason: format!("tool '{name}' is outside the '{role}' role's capability ceiling"),
        };
    }
    Decision::Allow
}

/// Return a red-line reason if this tool call crosses a global prohibition.
pub fn red_line_violation(tool: &str, args: &Value) -> Option<String> {
    match tool {
        "bash" => scan_command(args.get("command")?.as_str()?),
        "write_file" | "edit_file" => check_write_path(args.get("path")?.as_str()?),
        _ => None,
    }
}

// --- protected resources ---

/// Path fragments that name regin's own safety substrate / governance.
const PROTECTED_REGIN: &[&str] = &[
    ".local/share/regin",
    "regin.db",
    "kpi_events",
    "/usr/share/regin",
    "regind.service",
];

/// Path fragments that name backups/snapshots (never destroy a recovery point).
const PROTECTED_BACKUP: &[&str] = &["backup", "snapshot", ".snapshots", "/var/backups"];

/// Files whose modification subverts host authentication.
const PROTECTED_AUTH: &[&str] = &["/etc/shadow", "/etc/sudoers", "/etc/gshadow"];

/// Block creating/overwriting host-auth files via the file tools.
fn check_write_path(path: &str) -> Option<String> {
    let p = path.to_lowercase();
    if PROTECTED_AUTH.iter().any(|f| p.starts_with(f)) {
        return Some(format!("must not modify host authentication file {path}"));
    }
    if PROTECTED_REGIN.iter().any(|f| p.contains(f)) {
        return Some(format!("must not overwrite regin's own state/service ({path})"));
    }
    None
}

/// Split a shell line into simple commands on `;`, `&&`, `||`, `|`, and newlines.
/// Quote handling is intentionally naive (red-lines are heuristic).
fn split_simple(cmd: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let bytes: Vec<char> = cmd.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let two = if i + 1 < bytes.len() {
            Some((c, bytes[i + 1]))
        } else {
            None
        };
        match (c, two) {
            (_, Some(('&', '&'))) | (_, Some(('|', '|'))) if c == '&' || c == '|' => {
                out.push(std::mem::take(&mut cur));
                i += 2;
                continue;
            }
            (';', _) | ('|', _) | ('\n', _) => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
        i += 1;
    }
    out.push(cur);
    out.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

fn basename(tok: &str) -> &str {
    tok.rsplit('/').next().unwrap_or(tok)
}

fn scan_command(cmd: &str) -> Option<String> {
    let lower = cmd.to_lowercase();
    // Redirection straight onto a block device or auth file.
    if let Some(target) = redirect_target(&lower)
        && (target.starts_with("/dev/sd")
            || target.starts_with("/dev/nvme")
            || target.starts_with("/dev/vd")
            || PROTECTED_AUTH.iter().any(|f| target.starts_with(f)))
    {
        return Some(format!("redirection would clobber a device/auth target ({target})"));
    }

    for simple in split_simple(cmd) {
        let toks: Vec<String> = simple.split_whitespace().map(|t| t.to_lowercase()).collect();
        if toks.is_empty() {
            continue;
        }
        if let Some(reason) = check_simple(&toks) {
            return Some(reason);
        }
    }
    None
}

/// Extract the target of the last `>`/`>>` redirection, if any.
fn redirect_target(lower: &str) -> Option<String> {
    let idx = lower.rfind('>')?;
    let rest = lower[idx + 1..].trim_start_matches('>').trim();
    rest.split_whitespace().next().map(|s| s.to_string())
}

fn check_simple(toks: &[String]) -> Option<String> {
    let cmd = basename(&toks[0]);
    let args = &toks[1..];

    // Catastrophic filesystem destruction.
    if matches!(cmd, "rm" | "shred" | "unlink") {
        if args.iter().any(|a| is_root_or_home(a)) {
            return Some("must not recursively delete / or the home directory".into());
        }
        if let Some(p) = args.iter().find(|a| hits(a, PROTECTED_REGIN)) {
            return Some(format!("must not delete regin's own state/service ({p})"));
        }
        if let Some(p) = args.iter().find(|a| hits(a, PROTECTED_BACKUP)) {
            return Some(format!("must not delete a backup/snapshot ({p})"));
        }
    }
    if cmd == "dd" && args.iter().any(|a| a.starts_with("of=/dev/")) {
        return Some("must not dd onto a raw block device".into());
    }
    if cmd.starts_with("mkfs") {
        return Some("must not format a filesystem (mkfs)".into());
    }
    if cmd == "wipefs" && args.iter().any(|a| a == "-a" || a == "--all") {
        return Some("must not wipe filesystem signatures (wipefs -a)".into());
    }
    if matches!(cmd, "sfdisk" | "sgdisk") {
        return Some(format!("must not repartition the disk ({cmd})"));
    }

    // Severing governance: regin's own service / process.
    if cmd == "systemctl"
        && args.iter().any(|a| matches!(a.as_str(), "stop" | "disable" | "mask"))
        && args.iter().any(|a| unit_is_regin(a))
    {
        return Some("must not stop/disable regin's own service".into());
    }
    if matches!(cmd, "pkill" | "killall") && args.iter().any(|a| a.contains("regind")) {
        return Some("must not kill the regin daemon".into());
    }

    // Disabling the firewall wholesale.
    if cmd == "ufw" && args.iter().any(|a| a == "disable") {
        return Some("must not disable the firewall (ufw disable)".into());
    }
    if cmd == "systemctl"
        && args.iter().any(|a| matches!(a.as_str(), "stop" | "disable" | "mask"))
        && args.iter().any(|a| unit_is_firewall(a))
    {
        return Some("must not stop/disable the firewall service".into());
    }
    if matches!(cmd, "iptables" | "ip6tables")
        && args.iter().any(|a| a == "-f" || a == "--flush")
    {
        return Some("must not flush the firewall ruleset".into());
    }
    if cmd == "nft" && args.iter().any(|a| a == "flush") && args.iter().any(|a| a == "ruleset") {
        return Some("must not flush the nftables ruleset".into());
    }

    // Creating a root-equivalent account.
    if matches!(cmd, "useradd" | "adduser") && creates_uid_zero(args) {
        return Some("must not create a uid-0 (root-equivalent) account".into());
    }

    None
}

fn hits(arg: &str, fragments: &[&str]) -> bool {
    let a = arg.to_lowercase();
    fragments.iter().any(|f| a.contains(f))
}

fn is_root_or_home(arg: &str) -> bool {
    matches!(arg, "/" | "/*" | "/." | "~" | "~/" | "$home" | "${home}" | "$home/*")
}

fn unit_is_regin(arg: &str) -> bool {
    let a = arg.trim_end_matches(".service");
    a == "regind" || a == "regin"
}

fn unit_is_firewall(arg: &str) -> bool {
    let a = arg.trim_end_matches(".service");
    matches!(a, "firewalld" | "ufw" | "nftables" | "iptables" | "netfilter-persistent")
}

fn creates_uid_zero(args: &[String]) -> bool {
    // -u 0 / --uid 0 / -ou 0
    args.windows(2).any(|w| {
        (w[0] == "-u" || w[0] == "--uid" || w[0] == "-ou" || w[0] == "-o") && w[1] == "0"
    }) || args.iter().any(|a| a == "--uid=0" || a == "-u0")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{FunctionCall, ToolCall};

    fn bash(cmd: &str) -> Value {
        serde_json::json!({ "command": cmd })
    }

    fn rl(cmd: &str) -> Option<String> {
        red_line_violation("bash", &bash(cmd))
    }

    #[test]
    fn catastrophic_fs_is_blocked() {
        assert!(rl("rm -rf /").is_some());
        assert!(rl("rm -rf /*").is_some());
        assert!(rl("rm -rf ~").is_some());
        assert!(rl("dd if=/dev/zero of=/dev/sda").is_some());
        assert!(rl("mkfs.ext4 /dev/sdb1").is_some());
        assert!(rl("wipefs -a /dev/sda").is_some());
        assert!(rl("sgdisk --zap-all /dev/sda").is_some());
        assert!(rl("echo x > /dev/sda").is_some());
    }

    #[test]
    fn ordinary_remediation_is_allowed() {
        assert!(rl("rm -rf /tmp/cache/*").is_none());
        assert!(rl("rm -rf /var/lib/myapp/tmp").is_none());
        assert!(rl("systemctl restart nginx").is_none());
        assert!(rl("apt-get clean").is_none());
        assert!(rl("truncate -s 0 /var/log/app.log").is_none());
        assert!(rl(": > /var/log/syslog").is_none());
        assert!(rl("journalctl --vacuum-size=100M").is_none());
        assert!(rl("dd if=/dev/zero of=/tmp/testfile bs=1M count=10").is_none());
    }

    #[test]
    fn governance_and_substrate_protected() {
        assert!(rl("systemctl stop regind").is_some());
        assert!(rl("systemctl disable regind.service").is_some());
        assert!(rl("pkill regind").is_some());
        assert!(rl("rm -f ~/.local/share/regin/regin.db").is_some());
        assert!(rl("rm -rf /var/backups/nightly").is_some());
        assert!(rl("rm -rf /home/u/.snapshots").is_some());
        // restarting itself is fine (not stop/disable)
        assert!(rl("systemctl restart regind").is_none());
    }

    #[test]
    fn firewall_teardown_blocked() {
        assert!(rl("ufw disable").is_some());
        assert!(rl("systemctl stop firewalld").is_some());
        assert!(rl("iptables -F").is_some());
        assert!(rl("nft flush ruleset").is_some());
        // adding a rule is fine
        assert!(rl("iptables -A INPUT -p tcp --dport 22 -j ACCEPT").is_none());
        assert!(rl("ufw allow 22").is_none());
    }

    #[test]
    fn root_account_and_auth_files_protected() {
        assert!(rl("useradd -u 0 -o eviltwin").is_some());
        assert!(rl("useradd --uid=0 x").is_some());
        assert!(rl("echo bad >> /etc/shadow").is_some());
        assert!(red_line_violation("write_file", &serde_json::json!({"path": "/etc/shadow"})).is_some());
        assert!(red_line_violation("write_file", &serde_json::json!({"path": "/etc/sudoers.d/x"})).is_some());
        assert!(red_line_violation("edit_file", &serde_json::json!({"path": "/home/u/.local/share/regin/regin.db"})).is_some());
        // ordinary writes are fine
        assert!(red_line_violation("write_file", &serde_json::json!({"path": "/tmp/out.txt"})).is_none());
        assert!(rl("useradd normaluser").is_none());
    }

    #[test]
    fn red_line_hidden_in_a_chain_is_caught() {
        assert!(rl("cd /tmp && rm -rf / ; echo done").is_some());
        assert!(rl("ls | systemctl stop regind").is_some());
    }

    fn call(name: &str, args: Value) -> ToolCall {
        ToolCall {
            id: "1".into(),
            call_type: "function".into(),
            function: FunctionCall { name: name.into(), arguments: args.to_string() },
        }
    }

    #[test]
    fn red_line_beats_ceiling_and_names_the_layer() {
        let p = Persona::from_toml("role = \"operator\"\ntools = [\"read_file\"]\n").unwrap();

        // A red-line bash command: denied at the red-line layer even though the
        // ceiling would also have denied bash.
        let d = check_tool_call(&call("bash", bash("rm -rf /")), Some(&p));
        match &d {
            Decision::Deny { layer, .. } => assert_eq!(*layer, DecisionLayer::RedLine),
            _ => panic!("expected red-line denial"),
        }
        assert!(d.audit().unwrap().starts_with("[red-line]"));

        // A benign bash command outside the ceiling: denied at the ceiling layer.
        let d = check_tool_call(&call("bash", bash("ls /tmp")), Some(&p));
        match &d {
            Decision::Deny { layer, reason } => {
                assert_eq!(*layer, DecisionLayer::Ceiling);
                assert!(reason.contains("operator"));
            }
            _ => panic!("expected ceiling denial"),
        }

        // Within ceiling and no red-line: allowed.
        assert!(check_tool_call(&call("read_file", serde_json::json!({"path": "/tmp/x"})), Some(&p)).is_allowed());
    }

    #[test]
    fn unscoped_persona_still_hits_red_lines() {
        // No persona = unscoped ceiling, but red-lines still apply.
        let d = check_tool_call(&call("bash", bash("mkfs.ext4 /dev/sdb")), None);
        assert!(!d.is_allowed());
        assert_eq!(check_tool_call(&call("bash", bash("ls")), None), Decision::Allow);
    }
}
