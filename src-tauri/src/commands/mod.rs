pub mod clash;
pub mod core;
pub mod profile;
pub mod system;
pub mod verge;
pub mod window;

#[cfg(windows)]
pub mod service;

pub type CmdResult<T = ()> = Result<T, String>;
