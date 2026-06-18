//! Repo identity: resolve a working directory to a stable key used to scope
//! regin's per-repo additions (context, memories, skills) in its own store
//! (FEAT-008). The repo's filesystem path is the identifier.

use std::path::{Path, PathBuf};

fn canonical(p: &Path) -> String {
    std::fs::canonicalize(p)
        .unwrap_or_else(|_| p.to_path_buf())
        .to_string_lossy()
        .to_string()
}

/// Resolve `cwd` to a repo key: the canonical path of the nearest ancestor
/// containing a `.git` entry, else the canonical `cwd` itself. Returns `None`
/// when no working directory is known.
pub fn repo_key(cwd: Option<&str>) -> Option<String> {
    let dir = cwd?;
    let start = Path::new(dir);
    let mut cur: &Path = start;
    loop {
        if cur.join(".git").exists() {
            return Some(canonical(cur));
        }
        match cur.parent() {
            Some(parent) => cur = parent,
            None => break,
        }
    }
    Some(canonical(start))
}

/// Path to a legacy in-repo context file (`<cwd>/.repo/regin/context.md`),
/// retired by FEAT-008 but imported once if present.
pub fn legacy_context_path(cwd: Option<&str>) -> Option<PathBuf> {
    cwd.map(|d| Path::new(d).join(".repo").join("regin").join("context.md"))
}

/// Read the legacy in-repo context file, if it exists and is non-empty.
pub fn read_legacy_context(cwd: Option<&str>) -> Option<String> {
    let path = legacy_context_path(cwd)?;
    match std::fs::read_to_string(&path) {
        Ok(c) if !c.trim().is_empty() => Some(c),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir() -> PathBuf {
        let p = std::env::temp_dir().join(format!("regin-repo-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn repo_key_finds_git_root_from_nested_dir() {
        let root = tmpdir();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        let nested = root.join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();

        let key = repo_key(Some(nested.to_str().unwrap())).unwrap();
        assert_eq!(key, canonical(&root), "resolves to the git root, not the nested dir");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn repo_key_without_git_is_the_cwd() {
        let dir = tmpdir();
        let key = repo_key(Some(dir.to_str().unwrap())).unwrap();
        assert_eq!(key, canonical(&dir));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn repo_key_none_without_cwd() {
        assert!(repo_key(None).is_none());
    }

    #[test]
    fn read_legacy_context_roundtrips() {
        let dir = tmpdir();
        let cdir = dir.join(".repo").join("regin");
        std::fs::create_dir_all(&cdir).unwrap();
        std::fs::write(cdir.join("context.md"), "legacy ctx").unwrap();
        assert_eq!(read_legacy_context(Some(dir.to_str().unwrap())).as_deref(), Some("legacy ctx"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
