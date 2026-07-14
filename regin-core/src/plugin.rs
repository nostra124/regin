//! Plugin system (FEAT-082 / DISC-021): trait-based dylib plugins that hook
//! into the agent loop's lifecycle — `tool.execute.before`/`.after`,
//! `session.created`, `session.compacting`.
//!
//! **v1 scope, per the ticket**: compiled Rust `.so`/`.dylib` plugins loaded
//! at runtime via `libloading`, not a WASM sandbox (deferred to a follow-up
//! FEAT if demand warrants — see the ticket's closing note). This is a real,
//! documented trade-off, not an oversight: **Rust has no stable ABI**, so a
//! plugin dylib and the host must be built with the exact same rustc
//! version and the exact same version of this crate (`regin-core`) for
//! `Box<dyn Plugin>` to cross the FFI boundary safely. [`PLUGIN_API_VERSION`]
//! is the one safety net available for this — the host calls a plain
//! `extern "C" fn() -> u32` version-check symbol *before* ever calling the
//! riskier `Box<dyn Plugin>`-returning init symbol, so a version-skewed
//! plugin is rejected instead of invoked.
//!
//! **Layered like every other real-I/O integration in this crate**: hook
//! dispatch (ordering, panic isolation, reject short-circuiting) is
//! pure-ish and unit-tested against `Plugin` impls constructed directly in
//! Rust (no dylib needed); the dylib-loading path itself
//! (`PluginHost::load_dir`/`load_one`) is exercised against a *real*,
//! separately-compiled cdylib (the `test-plugin-fixture` workspace member)
//! — unlike FEAT-078's LSP client, a real plugin binary is genuinely
//! available in this build environment, so there's no need to fake this
//! layer too.

use anyhow::{Context, Result, bail};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// Bumped whenever [`Plugin`]'s shape changes in an ABI-incompatible way.
pub const PLUGIN_API_VERSION: u32 = 1;

type ApiVersionSymbol = unsafe extern "C" fn() -> u32;
// `Box<dyn Plugin>` isn't a stable C type (rustc warns `improper_ctypes_definitions`)
// — accepted deliberately, per the module doc comment: this is the exact
// signature the ticket specifies, and it's only ever called across the FFI
// boundary between binaries built by the same rustc + regin-core version,
// where the layout is stable in practice even though it isn't guaranteed by
// the language.
#[allow(improper_ctypes_definitions)]
type InitSymbol = unsafe extern "C" fn() -> Box<dyn Plugin>;

/// Pure version-mismatch check (acceptance criterion 7), decoupled from the
/// actual symbol lookup so it's directly unit-testable without a
/// specially-built mismatched fixture dylib.
fn check_version(declared: u32) -> Result<()> {
    if declared != PLUGIN_API_VERSION {
        bail!("was built for API version {declared}, host expects {PLUGIN_API_VERSION}");
    }
    Ok(())
}

/// The plugin trait (acceptance criterion 4). Every hook has a no-op
/// default so a plugin only needs to implement the ones it cares about.
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str {
        "unnamed-plugin"
    }

    /// `tool.execute.before` (criterion 2): runs before a tool call
    /// executes. Can pass the args through unmodified, rewrite them, or
    /// reject the call outright.
    fn on_tool_execute_before(&self, tool: &str, args: &str) -> ToolBeforeAction {
        let _ = tool;
        ToolBeforeAction::Continue { args: args.to_string() }
    }

    /// `tool.execute.after` (criterion 2): observes (and may rewrite) a
    /// tool's result before it's fed back to the LLM.
    fn on_tool_execute_after(&self, tool: &str, output: &str, success: bool) -> String {
        let _ = (tool, success);
        output.to_string()
    }

    /// `session.created` (criterion 2): a new chat session was opened.
    fn on_session_created(&self, session_id: &str) {
        let _ = session_id;
    }

    /// `session.compacting` (criterion 2): the session is being summarized
    /// for compaction; a plugin may inject additional context into the
    /// summary before it's stored.
    fn on_session_compacting(&self, summary: &str) -> String {
        summary.to_string()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolBeforeAction {
    Continue { args: String },
    Reject { reason: String },
}

struct LoadedPlugin {
    // Declared before `_lib` so it drops first — its vtable methods point
    // into that library's mapped code, so the plugin object must never
    // outlive the library that defines it.
    plugin: Box<dyn Plugin>,
    name: String,
    disabled: AtomicBool,
    /// `None` for a plugin constructed directly (tests) rather than loaded
    /// from a real dylib.
    _lib: Option<libloading::Library>,
}

/// Holds every loaded plugin and dispatches lifecycle hooks to them in load
/// order.
#[derive(Default)]
pub struct PluginHost {
    plugins: Mutex<Vec<LoadedPlugin>>,
}

impl PluginHost {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an in-process plugin directly (no dylib involved) — used by
    /// tests to exercise hook dispatch against a plain Rust `Plugin` impl
    /// without a real `.so`/`.dylib`, and available generally for any
    /// future in-process/built-in plugin.
    pub fn register(&self, name: &str, plugin: Box<dyn Plugin>) {
        self.plugins.lock().unwrap().push(LoadedPlugin { plugin, name: name.to_string(), disabled: AtomicBool::new(false), _lib: None });
    }

    /// Platform plugin file extension (acceptance criterion 3).
    pub fn plugin_extension() -> &'static str {
        if cfg!(target_os = "macos") {
            "dylib"
        } else if cfg!(target_os = "windows") {
            "dll"
        } else {
            "so"
        }
    }

    /// Scan `dir` for plugin dylibs and load each one that isn't
    /// explicitly disabled (criterion 6: `plugin.<name>.enabled`, default
    /// `true`). A missing directory yields no results, not an error — most
    /// installs have no plugins. Every file's outcome is independent
    /// (criterion 5): one bad plugin never stops the others.
    pub fn load_dir(&self, dir: &Path, conn: &rusqlite::Connection) -> Vec<(String, Result<()>)> {
        let ext = Self::plugin_extension();
        let mut results = Vec::new();
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return results,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some(ext) {
                continue;
            }
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("plugin").to_string();
            let enabled = crate::db::setting_get(conn, &format!("plugin.{name}.enabled")).map(|v| v != "false").unwrap_or(true);
            if !enabled {
                continue;
            }
            let outcome = self.load_one(&path, &name);
            results.push((name, outcome));
        }
        results
    }

    fn load_one(&self, path: &Path, name: &str) -> Result<()> {
        // SAFETY: loading and calling into an arbitrary dylib is inherently
        // unsafe — see the module doc comment for the accepted ABI-matching
        // risk this v1 plugin model carries.
        let lib = unsafe { libloading::Library::new(path) }.with_context(|| format!("loading plugin {path:?}"))?;

        let version = unsafe {
            let sym: libloading::Symbol<ApiVersionSymbol> = lib
                .get(b"regin_plugin_api_version\0")
                .with_context(|| format!("plugin {path:?} is missing regin_plugin_api_version"))?;
            sym()
        };
        check_version(version).with_context(|| format!("plugin {path:?}"))?;

        let init_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<Box<dyn Plugin>> {
            let sym: libloading::Symbol<InitSymbol> =
                unsafe { lib.get(b"regin_plugin_init\0") }.with_context(|| format!("plugin {path:?} is missing regin_plugin_init"))?;
            Ok(unsafe { sym() })
        }));
        let plugin = match init_result {
            Ok(Ok(p)) => p,
            Ok(Err(e)) => return Err(e),
            Err(_) => bail!("plugin {path:?} panicked during regin_plugin_init"),
        };

        self.plugins.lock().unwrap().push(LoadedPlugin { plugin, name: name.to_string(), disabled: AtomicBool::new(false), _lib: Some(lib) });
        Ok(())
    }

    pub fn loaded_names(&self) -> Vec<String> {
        self.plugins.lock().unwrap().iter().map(|p| p.name.clone()).collect()
    }

    #[cfg(test)]
    fn is_disabled(&self, name: &str) -> bool {
        self.plugins.lock().unwrap().iter().find(|p| p.name == name).map(|p| p.disabled.load(Ordering::SeqCst)).unwrap_or(false)
    }

    /// `tool.execute.before` (criteria 2, 5). Runs every enabled plugin in
    /// load order, threading each plugin's (possibly rewritten) args into
    /// the next. The first `Reject` short-circuits and wins. A plugin that
    /// panics is logged, disabled for the remainder of the session, and
    /// treated as a no-op for this call.
    pub fn tool_execute_before(&self, tool: &str, args: &str) -> ToolBeforeAction {
        let plugins = self.plugins.lock().unwrap();
        let mut current_args = args.to_string();
        for p in plugins.iter() {
            if p.disabled.load(Ordering::SeqCst) {
                continue;
            }
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| p.plugin.on_tool_execute_before(tool, &current_args))) {
                Ok(ToolBeforeAction::Reject { reason }) => return ToolBeforeAction::Reject { reason },
                Ok(ToolBeforeAction::Continue { args: new_args }) => current_args = new_args,
                Err(_) => {
                    tracing::warn!(plugin = %p.name, "panicked in tool.execute.before, disabling for the rest of the session");
                    p.disabled.store(true, Ordering::SeqCst);
                }
            }
        }
        ToolBeforeAction::Continue { args: current_args }
    }

    /// `tool.execute.after` (criteria 2, 5).
    pub fn tool_execute_after(&self, tool: &str, output: &str, success: bool) -> String {
        let plugins = self.plugins.lock().unwrap();
        let mut current = output.to_string();
        for p in plugins.iter() {
            if p.disabled.load(Ordering::SeqCst) {
                continue;
            }
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| p.plugin.on_tool_execute_after(tool, &current, success))) {
                Ok(new_output) => current = new_output,
                Err(_) => {
                    tracing::warn!(plugin = %p.name, "panicked in tool.execute.after, disabling for the rest of the session");
                    p.disabled.store(true, Ordering::SeqCst);
                }
            }
        }
        current
    }

    /// `session.created` (criteria 2, 5).
    pub fn session_created(&self, session_id: &str) {
        let plugins = self.plugins.lock().unwrap();
        for p in plugins.iter() {
            if p.disabled.load(Ordering::SeqCst) {
                continue;
            }
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| p.plugin.on_session_created(session_id))).is_err() {
                tracing::warn!(plugin = %p.name, "panicked in session.created, disabling for the rest of the session");
                p.disabled.store(true, Ordering::SeqCst);
            }
        }
    }

    /// `session.compacting` (criteria 2, 5).
    pub fn session_compacting(&self, summary: &str) -> String {
        let plugins = self.plugins.lock().unwrap();
        let mut current = summary.to_string();
        for p in plugins.iter() {
            if p.disabled.load(Ordering::SeqCst) {
                continue;
            }
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| p.plugin.on_session_compacting(&current))) {
                Ok(new_summary) => current = new_summary,
                Err(_) => {
                    tracing::warn!(plugin = %p.name, "panicked in session.compacting, disabling for the rest of the session");
                    p.disabled.store(true, Ordering::SeqCst);
                }
            }
        }
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> rusqlite::Connection {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::init_schema(&c).unwrap();
        c
    }

    // --- pure hook-dispatch logic, no real dylib needed --------------------

    struct RewritingPlugin;
    impl Plugin for RewritingPlugin {
        fn name(&self) -> &str {
            "rewriter"
        }
        fn on_tool_execute_before(&self, _tool: &str, args: &str) -> ToolBeforeAction {
            ToolBeforeAction::Continue { args: format!("{args}+rewritten") }
        }
        fn on_tool_execute_after(&self, _tool: &str, output: &str, _success: bool) -> String {
            format!("{output} [seen]")
        }
        fn on_session_compacting(&self, summary: &str) -> String {
            format!("{summary}\n(annotated)")
        }
    }

    struct RejectingPlugin;
    impl Plugin for RejectingPlugin {
        fn name(&self) -> &str {
            "rejector"
        }
        fn on_tool_execute_before(&self, tool: &str, args: &str) -> ToolBeforeAction {
            if tool == "blocked" {
                ToolBeforeAction::Reject { reason: "not allowed".into() }
            } else {
                ToolBeforeAction::Continue { args: args.to_string() }
            }
        }
    }

    struct PanickingPlugin;
    impl Plugin for PanickingPlugin {
        fn name(&self) -> &str {
            "panicker"
        }
        fn on_tool_execute_before(&self, _tool: &str, _args: &str) -> ToolBeforeAction {
            panic!("boom")
        }
        fn on_tool_execute_after(&self, _tool: &str, _output: &str, _success: bool) -> String {
            panic!("boom again")
        }
    }

    #[test]
    fn tool_execute_before_can_rewrite_args() {
        let host = PluginHost::new();
        host.register("rewriter", Box::new(RewritingPlugin));
        let action = host.tool_execute_before("bash", "echo hi");
        assert_eq!(action, ToolBeforeAction::Continue { args: "echo hi+rewritten".into() });
    }

    #[test]
    fn tool_execute_before_can_reject() {
        // acceptance criterion 2: "can modify args or reject"
        let host = PluginHost::new();
        host.register("rejector", Box::new(RejectingPlugin));
        let action = host.tool_execute_before("blocked", "{}");
        assert_eq!(action, ToolBeforeAction::Reject { reason: "not allowed".into() });

        let allowed = host.tool_execute_before("bash", "{}");
        assert_eq!(allowed, ToolBeforeAction::Continue { args: "{}".into() });
    }

    #[test]
    fn a_reject_short_circuits_before_later_plugins_run() {
        let host = PluginHost::new();
        host.register("rejector", Box::new(RejectingPlugin));
        host.register("rewriter", Box::new(RewritingPlugin));
        let action = host.tool_execute_before("blocked", "args");
        assert_eq!(action, ToolBeforeAction::Reject { reason: "not allowed".into() }, "rewriter never got a chance to run");
    }

    #[test]
    fn tool_execute_after_can_rewrite_output() {
        let host = PluginHost::new();
        host.register("rewriter", Box::new(RewritingPlugin));
        assert_eq!(host.tool_execute_after("bash", "hello", true), "hello [seen]");
    }

    #[test]
    fn session_compacting_can_inject_context() {
        let host = PluginHost::new();
        host.register("rewriter", Box::new(RewritingPlugin));
        assert_eq!(host.session_compacting("summary text"), "summary text\n(annotated)");
    }

    #[test]
    fn session_created_runs_without_panicking_on_a_no_op_default() {
        let host = PluginHost::new();
        host.register("rewriter", Box::new(RewritingPlugin));
        host.session_created("sess-1"); // default impl, just must not panic
    }

    #[test]
    fn a_panicking_plugin_is_disabled_and_treated_as_a_no_op_for_that_call() {
        // acceptance criteria 5, 7
        let host = PluginHost::new();
        host.register("panicker", Box::new(PanickingPlugin));
        host.register("rewriter", Box::new(RewritingPlugin));

        let action = host.tool_execute_before("bash", "echo hi");
        // panicker's panic is swallowed (treated as no-op) and rewriter
        // still runs afterward.
        assert_eq!(action, ToolBeforeAction::Continue { args: "echo hi+rewritten".into() });
        assert!(host.is_disabled("panicker"));
        assert!(!host.is_disabled("rewriter"));
    }

    #[test]
    fn a_disabled_plugin_is_skipped_on_every_subsequent_hook() {
        let host = PluginHost::new();
        host.register("panicker", Box::new(PanickingPlugin));

        let _ = host.tool_execute_before("bash", "x"); // triggers the panic, disables it
        assert!(host.is_disabled("panicker"));

        // Second call (a different hook on the same plugin) must not panic
        // again — it's skipped outright now.
        let output = host.tool_execute_after("bash", "result", true);
        assert_eq!(output, "result", "disabled plugin's on_tool_execute_after never runs");
    }

    #[test]
    fn plugin_extension_matches_the_current_platform() {
        let ext = PluginHost::plugin_extension();
        assert!(ext == "so" || ext == "dylib" || ext == "dll");
    }

    // --- dylib loading (acceptance criteria 3, 5, 6, 7) --------------------

    /// Ensures `test-plugin-fixture` has been built, then copies its cdylib
    /// into a fresh, otherwise-empty temp directory and returns (that
    /// directory, the fixture's plugin name). Loading from an isolated
    /// directory — rather than `target/debug` directly, which is full of
    /// unrelated `.so`s (proc-macro dylibs etc.) — keeps `load_dir` tests
    /// scoped to exactly the one plugin under test. Idempotent build:
    /// `cargo build` no-ops if already up to date, so this is correct
    /// regardless of whether `cargo test --workspace` already built every
    /// member first.
    fn fixture_plugin_dir() -> (tempfile_dir::TempDir, String) {
        let status = std::process::Command::new(env!("CARGO"))
            .args(["build", "-p", "test-plugin-fixture"])
            .status()
            .expect("failed to invoke cargo to build the plugin fixture");
        assert!(status.success(), "building test-plugin-fixture failed");

        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
        let filename = format!(
            "{}test_plugin_fixture.{}",
            if cfg!(target_os = "windows") { "" } else { "lib" },
            PluginHost::plugin_extension(),
        );
        let built_path = workspace_root.join("target").join("debug").join(&filename);
        assert!(built_path.exists(), "built fixture not found at {built_path:?}");

        let dir = tempfile_dir::TempDir::new();
        let dest = dir.path().join(&filename);
        std::fs::copy(&built_path, &dest).expect("copying fixture dylib into an isolated temp dir");
        let name = dest.file_stem().and_then(|s| s.to_str()).unwrap().to_string();
        (dir, name)
    }

    /// A minimal `TempDir`-alike (this crate has no dependency on the
    /// `tempfile` crate elsewhere): creates a unique directory under
    /// `std::env::temp_dir()` and removes it (and its contents) on drop.
    mod tempfile_dir {
        pub struct TempDir(std::path::PathBuf);
        impl TempDir {
            pub fn new() -> Self {
                let p = std::env::temp_dir().join(format!("regin-plugin-test-{}", uuid::Uuid::new_v4()));
                std::fs::create_dir_all(&p).unwrap();
                Self(p)
            }
            pub fn path(&self) -> &std::path::Path {
                &self.0
            }
        }
        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    #[test]
    fn loads_a_real_dylib_and_invokes_its_hooks() {
        // acceptance criteria 1, 3, 4: a genuinely separate, separately
        // compiled cdylib, loaded via libloading and driven through the
        // exact same hook-dispatch path as the in-process fakes above.
        let (dir, name) = fixture_plugin_dir();
        let c = conn();

        let host = PluginHost::new();
        let results = host.load_dir(dir.path(), &c);
        let fixture_result = results.iter().find(|(n, _)| *n == name);
        assert!(fixture_result.is_some(), "expected the fixture plugin among {results:?}");
        assert!(fixture_result.unwrap().1.is_ok(), "{:?}", fixture_result.unwrap().1);
        assert!(host.loaded_names().contains(&name));

        let action = host.tool_execute_before("blocked", "{}");
        assert_eq!(action, ToolBeforeAction::Reject { reason: "blocked by the fixture plugin".into() });

        let action = host.tool_execute_before("bash", "echo hi");
        assert_eq!(action, ToolBeforeAction::Continue { args: "echo hi+fixture".into() });

        let output = host.tool_execute_after("bash", "result", true);
        assert_eq!(output, "result [seen by fixture]");
    }

    #[test]
    fn a_disabled_plugin_setting_skips_loading_it() {
        // acceptance criterion 6
        let (dir, name) = fixture_plugin_dir();
        let c = conn();
        crate::db::setting_set(&c, &format!("plugin.{name}.enabled"), "false").unwrap();

        let host = PluginHost::new();
        let results = host.load_dir(dir.path(), &c);
        assert!(results.is_empty(), "disabled plugin should not even attempt to load: {results:?}");
        assert!(host.loaded_names().is_empty());
    }

    #[test]
    fn check_version_accepts_a_match_and_rejects_a_mismatch() {
        // acceptance criterion 7: version mismatch detection, checked
        // *before* the (unsafe, ABI-risky) init symbol would ever be
        // called — see load_one, which calls this exact function.
        assert!(check_version(PLUGIN_API_VERSION).is_ok());
        let err = check_version(PLUGIN_API_VERSION + 1).unwrap_err();
        assert!(err.to_string().contains("API version"), "{err}");
    }

    #[test]
    fn missing_plugin_directory_yields_no_results_not_an_error() {
        let host = PluginHost::new();
        let c = conn();
        let results = host.load_dir(std::path::Path::new("/no/such/plugins/dir"), &c);
        assert!(results.is_empty());
    }
}
