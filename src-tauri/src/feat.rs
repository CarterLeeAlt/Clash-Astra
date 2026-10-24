//! feat mod 里的函数主要用于
//! - hotkey 快捷键
//! - timer 定时器
//! - cmds 页面调用
//!
//! 实现按用途拆分在 [`crate::features`] 的子模块中，这里保留原有路径。
pub use crate::features::actions::*;
pub use crate::features::clash_config::*;
pub use crate::features::delay_test::*;
pub use crate::features::profile_update::*;
pub use crate::features::verge_config::*;
