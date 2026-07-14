fn main() {
    // FEAT-087: link against the system's libpam directly (hand-written FFI
    // bindings in webui::pam_auth — no bindgen/libclang dependency, unlike
    // the `pam`/`pam-sys` crates). Only needed when the `webui` feature is
    // enabled; `libpam0g-dev` (or equivalent) must be installed to link.
    if std::env::var("CARGO_FEATURE_WEBUI").is_ok() {
        println!("cargo:rustc-link-lib=pam");
    }
}
