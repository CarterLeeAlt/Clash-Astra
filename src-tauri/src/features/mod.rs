//! 功能入口的子模块，按用途拆分自原 `feat` 模块。
//! - actions：hotkey / tray 直接触发的动作
//! - clash_config / verge_config：配置修改
//! - profile_update：订阅更新与 core 配置重载
//! - delay_test：延迟测试与代理环境变量复制
pub mod actions;
pub mod clash_config;
pub mod delay_test;
pub mod profile_update;
pub mod verge_config;
