//! FEAT-014 (DISC-007): skill packages. regin's skills ship as **packages** that
//! dvalin deploys per role (dvalin FEAT-128): `regin-<base|role|area>-skills`. A
//! package is a directory with a `package.toml` manifest and a `skills/` tree of
//! individual skills (each a `<name>/skill.md`, matching `skills.rs`).
//!
//! ```text
//! regin-cfo-skills/
//!   package.toml          # name, kind, version, description
//!   skills/
//!     budget-review/skill.md
//!     month-end-close/skill.md
//! ```
//!
//! Installing a package copies its skills into the user skills dir (idempotent)
//! and records a marker so `packages` can list what's installed.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The `package.toml` manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageManifest {
    pub name: String,
    /// `base` | `role` | `area`.
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
}

fn default_kind() -> String {
    "role".to_string()
}

impl PackageManifest {
    pub fn from_toml(s: &str) -> Result<PackageManifest> {
        let m: PackageManifest = toml::from_str(s).context("parsing package.toml")?;
        if m.name.trim().is_empty() {
            anyhow::bail!("package manifest has an empty name");
        }
        Ok(m)
    }
}

/// A loaded skill package rooted at `dir`.
pub struct Package {
    pub manifest: PackageManifest,
    dir: PathBuf,
}

impl Package {
    /// Load a package from its directory (reads `package.toml`).
    pub fn load(dir: &Path) -> Result<Package> {
        let text = std::fs::read_to_string(dir.join("package.toml"))
            .with_context(|| format!("reading {}/package.toml", dir.display()))?;
        Ok(Package { manifest: PackageManifest::from_toml(&text)?, dir: dir.to_path_buf() })
    }

    /// The skills this package provides: `(name, skill_dir)` for every
    /// `skills/<name>/skill.md`.
    pub fn skills(&self) -> Result<Vec<(String, PathBuf)>> {
        let skills_root = self.dir.join("skills");
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(&skills_root) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e).with_context(|| format!("reading {}", skills_root.display())),
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.join("skill.md").is_file() {
                if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                    out.push((name.to_string(), p));
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }

    /// Install this package's skills into `user_skills_dir` (copies each
    /// `<name>/skill.md`). Idempotent — re-installing overwrites. Records a marker
    /// under `<user_skills_dir>/.packages/<name>.toml`. Returns installed skill names.
    pub fn install(&self, user_skills_dir: &Path) -> Result<Vec<String>> {
        std::fs::create_dir_all(user_skills_dir).ok();
        let mut installed = Vec::new();
        for (name, src) in self.skills()? {
            let dest = user_skills_dir.join(&name);
            std::fs::create_dir_all(&dest).ok();
            std::fs::copy(src.join("skill.md"), dest.join("skill.md"))
                .with_context(|| format!("installing skill {name}"))?;
            installed.push(name);
        }
        // marker so installed packages are listable
        let markers = user_skills_dir.join(".packages");
        std::fs::create_dir_all(&markers).ok();
        std::fs::write(
            markers.join(format!("{}.toml", self.manifest.name)),
            toml::to_string(&self.manifest).unwrap_or_default(),
        )?;
        Ok(installed)
    }
}

/// The packages installed into `user_skills_dir` (by manifest name).
pub fn installed_packages(user_skills_dir: &Path) -> Vec<String> {
    let markers = user_skills_dir.join(".packages");
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&markers) {
        for e in entries.flatten() {
            if let Some(stem) = e.path().file_stem().and_then(|s| s.to_str()) {
                out.push(stem.to_string());
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TmpDir(PathBuf);
    impl TmpDir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("regin-pkg-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&p).unwrap();
            TmpDir(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write(p: &Path, content: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    fn sample_pkg(root: &Path) {
        write(&root.join("package.toml"), "name = \"regin-cfo-skills\"\nkind = \"role\"\nversion = \"0.1.0\"\ndescription = \"CFO skills\"\n");
        write(&root.join("skills/budget-review/skill.md"), "Review the budget.\n");
        write(&root.join("skills/month-end-close/skill.md"), "Close the month.\n");
    }

    #[test]
    fn manifest_parses_and_validates() {
        let m = PackageManifest::from_toml("name = \"regin-base-skills\"\nkind = \"base\"\n").unwrap();
        assert_eq!(m.name, "regin-base-skills");
        assert_eq!(m.kind, "base");
        assert!(PackageManifest::from_toml("kind = \"role\"\n").is_err(), "missing name");
        // kind defaults to role
        assert_eq!(PackageManifest::from_toml("name = \"x\"\n").unwrap().kind, "role");
    }

    #[test]
    fn enumerates_package_skills() {
        let tmp = TmpDir::new();
        sample_pkg(tmp.path());
        let pkg = Package::load(tmp.path()).unwrap();
        let names: Vec<String> = pkg.skills().unwrap().into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, vec!["budget-review".to_string(), "month-end-close".to_string()]);
    }

    #[test]
    fn install_lands_skills_and_is_idempotent() {
        let tmp = TmpDir::new();
        let pkg_dir = tmp.path().join("pkg");
        let user_dir = tmp.path().join("user-skills");
        sample_pkg(&pkg_dir);
        let pkg = Package::load(&pkg_dir).unwrap();

        let installed = pkg.install(&user_dir).unwrap();
        assert_eq!(installed.len(), 2);
        assert_eq!(
            std::fs::read_to_string(user_dir.join("budget-review/skill.md")).unwrap(),
            "Review the budget.\n"
        );
        // re-install overwrites without error (idempotent)
        let again = pkg.install(&user_dir).unwrap();
        assert_eq!(again.len(), 2);
        // package is listed exactly once
        assert_eq!(installed_packages(&user_dir), vec!["regin-cfo-skills".to_string()]);
    }

    #[test]
    fn package_with_no_skills_dir_installs_nothing() {
        let tmp = TmpDir::new();
        write(&tmp.path().join("package.toml"), "name = \"regin-empty-skills\"\n");
        let pkg = Package::load(tmp.path()).unwrap();
        let user_dir = tmp.path().join("u");
        assert!(pkg.install(&user_dir).unwrap().is_empty());
        // marker still recorded
        assert_eq!(installed_packages(&user_dir), vec!["regin-empty-skills".to_string()]);
    }
}
