pub mod clash;
pub mod clash_settings;
pub mod core;
pub mod profile;
pub mod system;
pub mod window;

pub use clash::*;
pub use clash_settings::*;
pub use core::*;
pub use profile::*;
pub use system::*;
pub use window::*;

#[cfg(windows)]
pub mod service;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: &'static str,
    pub message: String,
    pub stage: Option<&'static str>,
    pub recovery_error: Option<String>,
}

impl From<anyhow::Error> for CommandError {
    fn from(error: anyhow::Error) -> Self {
        let failure = error.downcast_ref::<crate::application::transaction::OperationFailure>();
        Self {
            code: if error.is::<crate::application::budget::DeadlineExceeded>() {
                "OPERATION_TIMEOUT"
            } else if error.is::<crate::application::transaction::CompletionFailure>() {
                "APPLY_INCOMPLETE"
            } else if failure.is_some_and(|failure| failure.recovery_error.is_some()) {
                "RECOVERY_INCOMPLETE"
            } else if failure.is_some() {
                "CONFIGURATION_REJECTED"
            } else {
                "COMMAND_FAILED"
            },
            message: format!("{error:#}"),
            stage: failure.map(|failure| failure.stage),
            recovery_error: failure.and_then(|failure| failure.recovery_error.clone()),
        }
    }
}
impl From<String> for CommandError {
    fn from(message: String) -> Self {
        Self {
            code: "COMMAND_FAILED",
            message,
            stage: None,
            recovery_error: None,
        }
    }
}
impl From<&str> for CommandError {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

pub type CmdResult<T = ()> = Result<T, CommandError>;
