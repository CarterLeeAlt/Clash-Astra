use auto_launch::AutoLaunch;
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::sync::Arc;
use sysproxy::Sysproxy;
#[cfg(target_os = "windows")]
use sysproxy::WindowsProxySnapshot;

mod autolaunch;
mod guard;
mod proxy;

pub struct Sysopt {
    /// 本程序最近一次应用的系统代理设置。
    cur_sysproxy: Arc<Mutex<Option<Sysproxy>>>,

    /// 接管前的代理设置，用于退出恢复。
    old_sysproxy: Arc<Mutex<Option<Sysproxy>>>,

    /// 接管前的 Windows 注册表完整快照。
    #[cfg(target_os = "windows")]
    old_windows_proxy: Arc<Mutex<Option<WindowsProxySnapshot>>>,

    /// 接管后的注册表快照，用于核验恢复时的所有权。
    #[cfg(target_os = "windows")]
    owned_windows_proxy: Arc<Mutex<Option<WindowsProxySnapshot>>>,

    /// 系统自动启动项适配器。
    auto_launch: Arc<Mutex<Option<AutoLaunch>>>,

    /// 串行化代理写入、恢复与守卫刷新。
    proxy_operation: Arc<Mutex<()>>,
}

#[cfg(target_os = "windows")]
pub(crate) static DEFAULT_BYPASS: &str = "localhost;127.*;192.168.*;10.*;172.16.*;<local>";

impl Sysopt {
    pub fn global() -> &'static Sysopt {
        static SYSOPT: OnceCell<Sysopt> = OnceCell::new();

        SYSOPT.get_or_init(|| Sysopt {
            cur_sysproxy: Arc::new(Mutex::new(None)),
            old_sysproxy: Arc::new(Mutex::new(None)),
            #[cfg(target_os = "windows")]
            old_windows_proxy: Arc::new(Mutex::new(None)),
            #[cfg(target_os = "windows")]
            owned_windows_proxy: Arc::new(Mutex::new(None)),
            auto_launch: Arc::new(Mutex::new(None)),
            proxy_operation: Arc::new(Mutex::new(())),
        })
    }
}
