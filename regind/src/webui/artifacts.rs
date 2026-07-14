//! Public (unauthenticated by design — see `super`'s module doc comment)
//! static trees for build artifacts and package repositories (FEAT-087,
//! acceptance criteria 4 & 8): directory listing + file serving under
//! `webui.public.artifacts`/`webui.public.repo`, and best-effort APT/RPM
//! repo metadata regeneration.
//!
//! No `tower_http::services::ServeDir` here: that crate doesn't do
//! directory listing out of the box, and criterion 4 wants a browsable
//! tree, not just direct-file downloads — so this hand-rolls the small
//! amount of listing + file-serving logic instead of fighting the library
//! for something it isn't built to do.

use axum::extract::Path as AxumPath;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::WebuiState;

/// Registers the artifacts and repo trees at their *configured-at-startup*
/// URL prefixes. Prefixes are read once in `maybe_start` (not per-request)
/// — like `webui.port`, a change takes effect on the daemon's next
/// restart, the same convention `Request::WebuiDisable` documents.
pub fn router(artifacts_url: &str, artifacts_dir: PathBuf, repo_url: &str, repo_dir: PathBuf) -> Router<Arc<WebuiState>> {
    let mut router = Router::new();
    if !artifacts_url.is_empty() {
        let dir = artifacts_dir.clone();
        router = router.route(&format!("{artifacts_url}/{{*path}}"), get(move |AxumPath(p): AxumPath<String>| serve(dir.clone(), p)));
        let dir = artifacts_dir;
        router = router.route(artifacts_url, get(move || serve(dir.clone(), String::new())));
    }
    if !repo_url.is_empty() {
        let dir = repo_dir.clone();
        router = router.route(&format!("{repo_url}/{{*path}}"), get(move |AxumPath(p): AxumPath<String>| serve(dir.clone(), p)));
        let dir = repo_dir;
        router = router.route(repo_url, get(move || serve(dir.clone(), String::new())));
    }
    router
}

/// Rejects any relative path that would escape `base` (`..` segments) —
/// the one security-relevant check this hand-rolled server needs.
fn resolve_within(base: &Path, rel: &str) -> Option<PathBuf> {
    let mut resolved = base.to_path_buf();
    for segment in rel.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return None;
        }
        resolved.push(segment);
    }
    Some(resolved)
}

async fn serve(base: PathBuf, rel: String) -> Response {
    let Some(target) = resolve_within(&base, &rel) else {
        return (StatusCode::BAD_REQUEST, "invalid path").into_response();
    };

    let meta = match tokio::fs::metadata(&target).await {
        Ok(m) => m,
        Err(_) => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };

    if meta.is_dir() {
        list_directory(&target, &rel).await
    } else {
        serve_file(&target).await
    }
}

async fn list_directory(dir: &Path, rel: &str) -> Response {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(e) => e,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("failed to read directory: {e}")).into_response(),
    };

    let mut names = Vec::new();
    loop {
        match entries.next_entry().await {
            Ok(Some(entry)) => {
                let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
                let name = entry.file_name().to_string_lossy().into_owned();
                names.push((name, is_dir));
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }
    names.sort();

    let mut html = format!("<!doctype html><html><head><meta charset=\"utf-8\"><title>Index of /{rel}</title></head><body>");
    html.push_str(&format!("<h1>Index of /{rel}</h1><ul>"));
    if !rel.is_empty() {
        html.push_str("<li><a href=\"../\">../</a></li>");
    }
    for (name, is_dir) in names {
        let suffix = if is_dir { "/" } else { "" };
        html.push_str(&format!("<li><a href=\"{name}{suffix}\">{name}{suffix}</a></li>"));
    }
    html.push_str("</ul></body></html>");
    Html(html).into_response()
}

async fn serve_file(path: &Path) -> Response {
    let bytes = match tokio::fs::read(path).await {
        Ok(b) => b,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("failed to read file: {e}")).into_response(),
    };
    let content_type = guess_content_type(path);
    ([(axum::http::header::CONTENT_TYPE, content_type)], bytes).into_response()
}

fn guess_content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("deb") | Some("rpm") => "application/octet-stream",
        Some("gz") => "application/gzip",
        Some("json") => "application/json",
        Some("txt") | Some("asc") => "text/plain; charset=utf-8",
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        _ => "application/octet-stream",
    }
}

// ---------------------------------------------------------------------------
// Best-effort APT/RPM repo metadata regeneration (acceptance criterion 8)
// ---------------------------------------------------------------------------

/// Regenerates APT (`Packages`/`Packages.gz`) and RPM (`repodata/`) metadata
/// for `repo_dir`, using whichever of `apt-ftparchive`/`createrepo_c`/
/// `createrepo` are actually installed. Neither is a hard dependency of
/// this daemon or its packaging — an environment without them just skips
/// that half of the tree (logged, not an error): the raw `.deb`/`.rpm`
/// files are still served directly by [`router`] either way.
pub async fn regenerate_repo_metadata(repo_dir: &Path) {
    if !repo_dir.is_dir() {
        return;
    }
    if let Err(e) = regenerate_apt_metadata(repo_dir).await {
        tracing::info!("webui: APT repo metadata not regenerated: {e:#}");
    }
    if let Err(e) = regenerate_rpm_metadata(repo_dir).await {
        tracing::info!("webui: RPM repo metadata not regenerated: {e:#}");
    }
}

async fn regenerate_apt_metadata(repo_dir: &Path) -> anyhow::Result<()> {
    which("apt-ftparchive").await?;
    let output = tokio::process::Command::new("apt-ftparchive").arg("packages").arg(".").current_dir(repo_dir).output().await?;
    if !output.status.success() {
        anyhow::bail!("apt-ftparchive exited with {}", output.status);
    }
    tokio::fs::write(repo_dir.join("Packages"), &output.stdout).await?;
    Ok(())
}

async fn regenerate_rpm_metadata(repo_dir: &Path) -> anyhow::Result<()> {
    let tool = if which("createrepo_c").await.is_ok() {
        "createrepo_c"
    } else {
        which("createrepo").await?;
        "createrepo"
    };
    let status = tokio::process::Command::new(tool).arg(".").current_dir(repo_dir).status().await?;
    if !status.success() {
        anyhow::bail!("{tool} exited with {status}");
    }
    Ok(())
}

async fn which(bin: &str) -> anyhow::Result<()> {
    let status = tokio::process::Command::new("which").arg(bin).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().await?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("{bin} not found in PATH")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_within_rejects_parent_traversal() {
        let base = Path::new("/srv/artifacts");
        assert_eq!(resolve_within(base, "../etc/passwd"), None);
        assert_eq!(resolve_within(base, "sub/../../etc/passwd"), None);
    }

    #[test]
    fn resolve_within_accepts_plain_relative_paths() {
        let base = Path::new("/srv/artifacts");
        assert_eq!(resolve_within(base, "builds/regind-0.1.0.deb"), Some(PathBuf::from("/srv/artifacts/builds/regind-0.1.0.deb")));
        assert_eq!(resolve_within(base, ""), Some(PathBuf::from("/srv/artifacts")));
    }

    #[tokio::test]
    async fn serve_lists_a_real_directory() {
        let dir = tempdir();
        tokio::fs::write(dir.join("a.txt"), b"hello").await.unwrap();
        tokio::fs::create_dir(dir.join("sub")).await.unwrap();

        let resp = serve(dir.clone(), String::new()).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(body.contains("a.txt"));
        assert!(body.contains("sub/"));

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn serve_returns_file_bytes_for_a_real_file() {
        let dir = tempdir();
        tokio::fs::write(dir.join("a.txt"), b"hello world").await.unwrap();

        let resp = serve(dir.clone(), "a.txt".to_string()).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&bytes[..], b"hello world");

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn serve_404s_on_a_missing_path() {
        let dir = tempdir();
        let resp = serve(dir.clone(), "nope.txt".to_string()).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn serve_rejects_path_traversal_with_bad_request() {
        let dir = tempdir();
        let resp = serve(dir.clone(), "../../etc/passwd".to_string()).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn regenerate_repo_metadata_degrades_gracefully_without_the_tools() {
        // Doesn't assert on presence/absence of apt-ftparchive/createrepo in
        // this environment (sandbox-dependent) — only that it never panics
        // or hangs when a repo dir has no matching packages.
        let dir = tempdir();
        regenerate_repo_metadata(&dir).await;
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("regin-webui-artifacts-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
