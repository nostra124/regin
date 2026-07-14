//! A real, separately-compiled plugin dylib used only by
//! `regin_core::plugin`'s tests — proves the trait-based dylib plugin
//! loading mechanism (FEAT-082) actually works end-to-end, not just against
//! in-process fakes.

use regin_core::plugin::{PLUGIN_API_VERSION, Plugin, ToolBeforeAction};

struct FixturePlugin;

impl Plugin for FixturePlugin {
    fn name(&self) -> &str {
        "test-plugin-fixture"
    }

    fn on_tool_execute_before(&self, tool: &str, args: &str) -> ToolBeforeAction {
        if tool == "blocked" {
            ToolBeforeAction::Reject { reason: "blocked by the fixture plugin".into() }
        } else {
            ToolBeforeAction::Continue { args: format!("{args}+fixture") }
        }
    }

    fn on_tool_execute_after(&self, _tool: &str, output: &str, _success: bool) -> String {
        format!("{output} [seen by fixture]")
    }

    fn on_session_compacting(&self, summary: &str) -> String {
        format!("{summary}\n(fixture plugin was here)")
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn regin_plugin_api_version() -> u32 {
    PLUGIN_API_VERSION
}

#[unsafe(no_mangle)]
#[allow(improper_ctypes_definitions)]
pub extern "C" fn regin_plugin_init() -> Box<dyn Plugin> {
    Box::new(FixturePlugin)
}
