#[path = "../../src/service_protocol.rs"]
mod protocol;
pub use protocol::*;

pub fn api_token_path() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|path| path.join(API_TOKEN_DIR).join(API_TOKEN_FILE))
}

pub fn remove_token_file_and_empty_parent(path: &std::path::Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::remove_dir(parent);
    }
    Ok(())
}
#[path = "../../src/infrastructure/process_control.rs"]
pub mod process_control;
