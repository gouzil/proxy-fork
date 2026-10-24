mod github;
mod install;
mod verify;
pub use github::*;
pub use install::*;
pub use verify::*;

pub struct VerifiedUpdate {
    pub directory: tempfile::TempDir,
    pub bundle: std::path::PathBuf,
    pub version: String,
}

pub fn target() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin"
    } else {
        "x86_64-apple-darwin"
    }
}
