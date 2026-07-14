//! The two public (unauthenticated) HTML surfaces (FEAT-087, acceptance
//! criteria 4 & 10): a minimal landing page at `/`, and the single-file,
//! no-build-step mobile SPA at `/regin/` (chat/terminal/goal tabs, dynamic
//! dashboard tabs, login form — see `assets/spa.html`). Both are embedded
//! into the binary with `include_str!` at compile time, so there's no
//! runtime filesystem dependency and no separate asset pipeline.

use axum::response::Html;

const LANDING_HTML: &str = include_str!("assets/landing.html");
const SPA_HTML: &str = include_str!("assets/spa.html");

pub async fn landing_page() -> Html<&'static str> {
    Html(LANDING_HTML)
}

pub async fn spa_shell() -> Html<&'static str> {
    Html(SPA_HTML)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn landing_page_serves_non_empty_html() {
        let Html(body) = landing_page().await;
        assert!(body.contains("<html"));
    }

    #[tokio::test]
    async fn spa_shell_serves_the_login_form_and_all_three_core_tabs() {
        let Html(body) = spa_shell().await;
        assert!(body.contains("id=\"login-form\""));
        assert!(body.contains("Chat"));
        assert!(body.contains("Terminal"));
        assert!(body.contains("Goal"));
    }
}
