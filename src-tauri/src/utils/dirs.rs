use crate::core::handle;
use crate::service_protocol::{API_TOKEN_DIR, API_TOKEN_FILE, FIREWALL_HELPER, SERVICE_BINARY};
use anyhow::Result;
use std::path::PathBuf;
use tauri::{api::path::resource_dir, Env};

#[cfg(not(feature = "verge-dev"))]
pub static APP_ID: &str = "io.github.clash-astra";
#[cfg(feature = "verge-dev")]
pub static APP_ID: &str = "io.github.clash-astra.dev";

static CLASH_CONFIG: &str = "config.yaml";
static VERGE_CONFIG: &str = "verge.yaml";
static PROFILE_YAML: &str = "profiles.yaml";

/// 当前可执行文件所在目录；程序创建的数据均放在该目录树中。
pub fn executable_dir() -> Result<PathBuf> {
    use tauri::utils::platform::current_exe;

    let app_exe = dunce::canonicalize(current_exe()?)?;
    app_exe
        .parent()
        .map(PathBuf::from)
        .ok_or(anyhow::anyhow!("failed to get the executable directory"))
}

/// 应用配置根目录。
pub fn app_home_dir() -> Result<PathBuf> {
    Ok(executable_dir()?.join(".config").join(APP_ID))
}

/// WebView 缓存、Cookie、LocalStorage 和 IndexedDB 的存储目录。
pub fn webview_data_dir() -> Result<PathBuf> {
    Ok(app_home_dir()?.join("webview"))
}

/// 本应用管理的临时文件目录。
pub fn app_temp_dir() -> Result<PathBuf> {
    Ok(app_home_dir()?.join("temp"))
}

/// Tauri 打包资源目录。
pub fn app_resources_dir() -> Result<PathBuf> {
    let handle = handle::Handle::global();
    let app_handle = handle.app_handle.lock();
    if let Some(app_handle) = app_handle.as_ref() {
        let res_dir = resource_dir(app_handle.package_info(), &Env::default())
            .ok_or(anyhow::anyhow!("failed to get the resource dir"))?
            .join("resources");
        return Ok(res_dir);
    };
    Err(anyhow::anyhow!("failed to get the resource dir"))
}

/// 订阅文件目录。
pub fn app_profiles_dir() -> Result<PathBuf> {
    Ok(app_home_dir()?.join("profiles"))
}

/// 日志根目录。
pub fn logs_dir() -> Result<PathBuf> {
    Ok(app_home_dir()?.join("logs"))
}

/// 应用日志目录。
pub fn app_logs_dir() -> Result<PathBuf> {
    let dir = logs_dir()?.join("app");
    let _ = std::fs::create_dir_all(&dir);
    Ok(dir)
}

/// Windows 服务日志目录。
pub fn service_logs_dir() -> Result<PathBuf> {
    let dir = logs_dir()?.join("service");
    let _ = std::fs::create_dir_all(&dir);
    Ok(dir)
}

pub fn clash_path() -> Result<PathBuf> {
    Ok(app_home_dir()?.join(CLASH_CONFIG))
}

pub fn verge_path() -> Result<PathBuf> {
    Ok(app_home_dir()?.join(VERGE_CONFIG))
}

pub fn profiles_path() -> Result<PathBuf> {
    Ok(app_home_dir()?.join(PROFILE_YAML))
}

pub fn clash_pid_path() -> Result<PathBuf> {
    Ok(app_home_dir()?.join("clash.pid"))
}

#[cfg(windows)]
pub fn service_path() -> Result<PathBuf> {
    Ok(app_resources_dir()?.join(SERVICE_BINARY))
}

#[cfg(windows)]
pub fn firewall_helper_path() -> Result<PathBuf> {
    Ok(app_resources_dir()?.join(FIREWALL_HELPER))
}

#[cfg(windows)]
pub fn service_api_token_path() -> Result<PathBuf> {
    Ok(app_resources_dir()?
        .join(API_TOKEN_DIR)
        .join(API_TOKEN_FILE))
}

#[cfg(windows)]
pub fn service_log_file() -> Result<PathBuf> {
    use chrono::Local;

    let log_dir = service_logs_dir()?;

    let local_time = Local::now().format("%Y-%m-%d-%H%M").to_string();
    let log_file = format!("{}.log", local_time);
    let log_file = log_dir.join(log_file);

    Ok(log_file)
}

pub fn path_to_str(path: &PathBuf) -> Result<&str> {
    let path_str = path
        .as_os_str()
        .to_str()
        .ok_or(anyhow::anyhow!("failed to get path from {:?}", path))?;
    Ok(path_str)
}
