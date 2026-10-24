pub use crate::commands::clash::*;
pub use crate::commands::core::*;
pub use crate::commands::profile::*;
pub use crate::commands::system::*;
pub use crate::commands::verge::*;
pub use crate::commands::window::*;
pub use crate::commands::CmdResult;

#[cfg(windows)]
pub use crate::commands::service;

use crate::wrap_err;

#[cfg(windows)]
pub mod uwp {
    use super::*;
    use crate::core::win_uwp;

    #[tauri::command]
    pub async fn invoke_uwp_tool() -> CmdResult {
        wrap_err!(win_uwp::invoke_uwptools().await)
    }
}
