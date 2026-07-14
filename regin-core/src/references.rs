//! External references (FEAT-084 / DISC-021): additional directories — local
//! paths or shallow-cloned git repos — injected into the system prompt so
//! the agent can `read_file`/`glob` into them without leaving the session.
//!
//! **Layered like every other real-I/O integration in this crate**:
//! configuration discovery, path resolution, and prompt rendering are pure
//! and unit-tested directly; the one real-I/O piece (`git clone`) sits
//! behind the [`RepoCloner`] trait so the orchestration (`resolve_reference`)
//! is testable with a fake — a real network clone is neither reliable nor
//! desirable to run in a test suite.

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use std::path::{Path, PathBuf};

/// One configured reference (acceptance criteria 1, 3, 6).
#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceConfig {
    pub alias: String,
    pub path: Option<String>,
    pub repository: Option<String>,
    pub branch: Option<String>,
    pub description: Option<String>,
}

/// A reference resolved to a local, readable directory.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedReference {
    pub alias: String,
    pub local_dir: PathBuf,
    pub description: Option<String>,
}

/// Discover every configured reference (`references.<alias>.path` or
/// `references.<alias>.repository` present in settings) and resolve its
/// config. Each alias's resolution is independent — a malformed reference
/// doesn't prevent discovering the others (same fail-safe-per-item
/// convention as `mcp::discover_configured_servers`).
pub fn discover_configured_references(conn: &rusqlite::Connection) -> Vec<(String, Result<ReferenceConfig>)> {
    let all = crate::db::setting_list(conn).unwrap_or_default();
    let mut aliases: Vec<String> = all
        .iter()
        .filter_map(|(k, _)| {
            k.strip_prefix("references.").and_then(|rest| rest.strip_suffix(".path").or_else(|| rest.strip_suffix(".repository")))
        })
        .map(str::to_string)
        .collect();
    aliases.sort();
    aliases.dedup();
    aliases.into_iter().map(|alias| { let cfg = resolve_reference_config(conn, &alias); (alias, cfg) }).collect()
}

fn resolve_reference_config(conn: &rusqlite::Connection, alias: &str) -> Result<ReferenceConfig> {
    let non_empty = |key: String| -> Result<Option<String>> {
        let v = crate::db::setting_get(conn, &key)?;
        Ok(if v.trim().is_empty() { None } else { Some(v) })
    };
    let path = non_empty(format!("references.{alias}.path"))?;
    let repository = non_empty(format!("references.{alias}.repository"))?;
    if path.is_none() && repository.is_none() {
        // Also how "removal" works (acceptance criterion 7): clearing both
        // settings to empty makes the alias fail to resolve, dropping out
        // of the active reference list — there's no separate delete verb,
        // consistent with this codebase's settings model (no DELETE, only
        // overwrite).
        bail!("references.{alias} has neither a path nor a repository configured");
    }
    let branch = non_empty(format!("references.{alias}.branch"))?;
    let description = non_empty(format!("references.{alias}.description"))?;
    Ok(ReferenceConfig { alias: alias.to_string(), path, repository, branch, description })
}

/// Resolve a raw configured path (acceptance criterion 7): `~/...` expands
/// against `home_dir`; an absolute path is used as-is; a bare relative path
/// is returned unchanged (resolved against the caller's own working
/// directory the normal way, same as any other relative path in Rust —
/// there is no reference-specific base to resolve it against).
pub fn resolve_path(raw: &str, home_dir: Option<&Path>) -> PathBuf {
    if raw == "~" {
        if let Some(home) = home_dir {
            return home.to_path_buf();
        }
    } else if let Some(rest) = raw.strip_prefix("~/")
        && let Some(home) = home_dir
    {
        return home.join(rest);
    }
    PathBuf::from(raw)
}

/// Clones (or otherwise fetches) a `repository` reference into `dest`.
/// Implemented in `regin_core` behind a trait so `resolve_reference`'s
/// orchestration is testable without a real `git`/network call.
#[async_trait]
pub trait RepoCloner: Send + Sync {
    async fn ensure_cloned(&self, repository: &str, branch: Option<&str>, dest: &Path) -> Result<()>;
}

/// The real cloner: a shallow (`--depth 1`) `git clone`. v1 scope,
/// documented rather than hidden: if `dest` already exists, it's assumed
/// to be a valid prior clone and is used as-is — there's no refresh/pull
/// here, so a reference's content is whatever it was the first time it was
/// resolved until the cache directory is removed by hand. Acceptable for a
/// read-only reference; revisiting this (e.g. a `references.<alias>.refresh`
/// setting) is a natural follow-up if it turns out to matter in practice.
pub struct GitRepoCloner;

#[async_trait]
impl RepoCloner for GitRepoCloner {
    async fn ensure_cloned(&self, repository: &str, branch: Option<&str>, dest: &Path) -> Result<()> {
        if dest.exists() {
            return Ok(());
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).with_context(|| format!("creating {parent:?}"))?;
        }
        let url = repo_url(repository);
        let mut cmd = tokio::process::Command::new("git");
        cmd.arg("clone").arg("--depth").arg("1").arg("--quiet");
        if let Some(b) = branch {
            cmd.arg("--branch").arg(b);
        }
        cmd.arg(&url).arg(dest);
        let status = cmd.status().await.with_context(|| format!("running git clone {url}"))?;
        if !status.success() {
            bail!("git clone failed for {repository} ({url})");
        }
        Ok(())
    }
}

/// A short `owner/repo` reference clones from GitHub by default; anything
/// that already looks like a URL (`http(s)://`, `git@`) is used verbatim,
/// so a self-hosted GitLab/Gitea reference works too.
fn repo_url(repository: &str) -> String {
    if repository.starts_with("http://") || repository.starts_with("https://") || repository.starts_with("git@") {
        repository.to_string()
    } else {
        format!("https://github.com/{repository}.git")
    }
}

/// Resolve one configured reference to a local, readable directory
/// (acceptance criteria 2, 3): a `path` reference resolves directly; a
/// `repository` reference is cloned (via `cloner`) into
/// `cache_dir/<alias>` if not already present.
pub async fn resolve_reference(
    config: &ReferenceConfig,
    cache_dir: &Path,
    cloner: &dyn RepoCloner,
    home_dir: Option<&Path>,
) -> Result<ResolvedReference> {
    let local_dir = if let Some(path) = &config.path {
        resolve_path(path, home_dir)
    } else if let Some(repository) = &config.repository {
        let dest = cache_dir.join(&config.alias);
        cloner.ensure_cloned(repository, config.branch.as_deref(), &dest).await?;
        dest
    } else {
        bail!("reference {} has neither a path nor a repository", config.alias);
    };
    Ok(ResolvedReference { alias: config.alias.clone(), local_dir, description: config.description.clone() })
}

/// Render every resolved reference into a system-prompt block (acceptance
/// criterion 4): the agent sees each reference's alias, local directory,
/// and optional description, and is told it can `read_file`/`glob` into
/// them directly. `None` when there are no references — nothing is added
/// to the prompt for an install that hasn't configured any.
pub fn render_references_context(refs: &[ResolvedReference]) -> Option<String> {
    if refs.is_empty() {
        return None;
    }
    let mut out = String::from(
        "## External references\n\nThese additional directories are available — read files from them with `read_file`/`glob` using the paths below:\n",
    );
    for r in refs {
        let dir = r.local_dir.display();
        match &r.description {
            Some(d) => out.push_str(&format!("- **{}** (`{dir}`): {d}\n", r.alias)),
            None => out.push_str(&format!("- **{}** (`{dir}`)\n", r.alias)),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> rusqlite::Connection {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::init_schema(&c).unwrap();
        c
    }

    // --- criteria 1, 6: configuration discovery ----------------------------

    #[test]
    fn discovers_a_path_reference() {
        let c = conn();
        crate::db::setting_set(&c, "references.helpers.path", "/opt/helpers").unwrap();
        crate::db::setting_set(&c, "references.helpers.description", "shared utility functions").unwrap();

        let discovered = discover_configured_references(&c);
        assert_eq!(discovered.len(), 1);
        let (alias, cfg) = &discovered[0];
        assert_eq!(alias, "helpers");
        let cfg = cfg.as_ref().unwrap();
        assert_eq!(cfg.path.as_deref(), Some("/opt/helpers"));
        assert_eq!(cfg.repository, None);
        assert_eq!(cfg.description.as_deref(), Some("shared utility functions"));
    }

    #[test]
    fn discovers_a_repository_reference_with_a_branch() {
        let c = conn();
        crate::db::setting_set(&c, "references.docs.repository", "example/docs").unwrap();
        crate::db::setting_set(&c, "references.docs.branch", "stable").unwrap();

        let discovered = discover_configured_references(&c);
        let (alias, cfg) = &discovered[0];
        assert_eq!(alias, "docs");
        let cfg = cfg.as_ref().unwrap();
        assert_eq!(cfg.repository.as_deref(), Some("example/docs"));
        assert_eq!(cfg.branch.as_deref(), Some("stable"));
        assert_eq!(cfg.path, None);
    }

    #[test]
    fn a_malformed_reference_does_not_prevent_discovering_others() {
        let c = conn();
        // "broken" only has a branch set, no path/repository -> errors.
        crate::db::setting_set(&c, "references.broken.branch", "main").unwrap();
        crate::db::setting_set(&c, "references.ok.path", "/tmp/ok").unwrap();

        let discovered = discover_configured_references(&c);
        assert_eq!(discovered.len(), 1, "only aliases with a path or repository key are discovered at all: {discovered:?}");
        assert_eq!(discovered[0].0, "ok");
        assert!(discovered[0].1.is_ok());
    }

    #[test]
    fn no_configured_references_discovers_nothing() {
        let c = conn();
        assert!(discover_configured_references(&c).is_empty());
    }

    #[test]
    fn removal_is_clearing_both_settings_to_empty() {
        // criterion 7: "reference removal" — there's no delete verb in this
        // codebase's settings model, only overwrite; emptying both keys
        // makes the alias fail to resolve, i.e. it drops out of the active
        // reference list.
        let c = conn();
        crate::db::setting_set(&c, "references.gone.path", "/tmp/x").unwrap();
        assert_eq!(discover_configured_references(&c).len(), 1);

        crate::db::setting_set(&c, "references.gone.path", "").unwrap();
        let discovered = discover_configured_references(&c);
        assert_eq!(discovered.len(), 1, "the key still exists in settings, so it's still discovered...");
        assert!(discovered[0].1.is_err(), "...but resolution now fails, since neither path nor repository is set");
    }

    // --- criterion 7: path resolution (relative, absolute, home-dir) -------

    #[test]
    fn resolve_path_expands_home_dir() {
        let home = Path::new("/home/rene");
        assert_eq!(resolve_path("~", Some(home)), PathBuf::from("/home/rene"));
        assert_eq!(resolve_path("~/projects/helpers", Some(home)), PathBuf::from("/home/rene/projects/helpers"));
    }

    #[test]
    fn resolve_path_leaves_absolute_paths_unchanged() {
        assert_eq!(resolve_path("/opt/helpers", Some(Path::new("/home/rene"))), PathBuf::from("/opt/helpers"));
    }

    #[test]
    fn resolve_path_leaves_relative_paths_unchanged() {
        assert_eq!(resolve_path("relative/dir", Some(Path::new("/home/rene"))), PathBuf::from("relative/dir"));
    }

    #[test]
    fn resolve_path_without_a_known_home_dir_falls_back_to_the_raw_string() {
        assert_eq!(resolve_path("~/x", None), PathBuf::from("~/x"), "no home dir known -> can't expand, pass through as-is");
    }

    // --- criterion 2: repo URL derivation -----------------------------------

    #[test]
    fn repo_url_defaults_to_github_and_passes_through_full_urls() {
        assert_eq!(repo_url("example/helpers"), "https://github.com/example/helpers.git");
        assert_eq!(repo_url("https://gitlab.example/helpers.git"), "https://gitlab.example/helpers.git");
        assert_eq!(repo_url("git@github.com:example/helpers.git"), "git@github.com:example/helpers.git");
    }

    // --- criteria 2, 3: resolve_reference orchestration (fake cloner) ------

    struct FakeCloner {
        calls: std::sync::Mutex<Vec<(String, Option<String>, PathBuf)>>,
    }
    impl FakeCloner {
        fn new() -> Self {
            Self { calls: std::sync::Mutex::new(Vec::new()) }
        }
    }
    #[async_trait]
    impl RepoCloner for FakeCloner {
        async fn ensure_cloned(&self, repository: &str, branch: Option<&str>, dest: &Path) -> Result<()> {
            self.calls.lock().unwrap().push((repository.to_string(), branch.map(str::to_string), dest.to_path_buf()));
            Ok(())
        }
    }

    #[tokio::test]
    async fn a_path_reference_resolves_without_touching_the_cloner() {
        let cloner = FakeCloner::new();
        let cfg = ReferenceConfig { alias: "helpers".into(), path: Some("/opt/helpers".into()), repository: None, branch: None, description: None };
        let resolved = resolve_reference(&cfg, Path::new("/cache"), &cloner, None).await.unwrap();
        assert_eq!(resolved.local_dir, PathBuf::from("/opt/helpers"));
        assert!(cloner.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_repository_reference_is_cloned_into_the_cache_dir_under_its_alias() {
        let cloner = FakeCloner::new();
        let cfg = ReferenceConfig { alias: "docs".into(), path: None, repository: Some("example/docs".into()), branch: Some("stable".into()), description: Some("API docs".into()) };
        let resolved = resolve_reference(&cfg, Path::new("/cache"), &cloner, None).await.unwrap();
        assert_eq!(resolved.local_dir, PathBuf::from("/cache/docs"));
        assert_eq!(resolved.description.as_deref(), Some("API docs"));
        assert_eq!(*cloner.calls.lock().unwrap(), vec![("example/docs".to_string(), Some("stable".to_string()), PathBuf::from("/cache/docs"))]);
    }

    // --- criterion 4: prompt rendering --------------------------------------

    #[test]
    fn render_context_is_none_with_no_references() {
        assert_eq!(render_references_context(&[]), None);
    }

    #[test]
    fn render_context_lists_alias_dir_and_description() {
        let refs = vec![
            ResolvedReference { alias: "helpers".into(), local_dir: PathBuf::from("/opt/helpers"), description: Some("shared utils".into()) },
            ResolvedReference { alias: "docs".into(), local_dir: PathBuf::from("/cache/docs"), description: None },
        ];
        let rendered = render_references_context(&refs).unwrap();
        assert!(rendered.contains("helpers"));
        assert!(rendered.contains("/opt/helpers"));
        assert!(rendered.contains("shared utils"));
        assert!(rendered.contains("docs"));
        assert!(rendered.contains("/cache/docs"));
        assert!(rendered.contains("read_file"));
    }
}
