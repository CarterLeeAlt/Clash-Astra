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
    /// current system proxy setting
    cur_sysproxy: Arc<Mutex<Option<Sysproxy>>>,

    /// record the original system proxy
    /// recover it when exit
    old_sysproxy: Arc<Mutex<Option<Sysproxy>>>,

    /// exact Windows registry state before this app owns the proxy
    #[cfg(target_os = "windows")]
    old_windows_proxy: Arc<Mutex<Option<WindowsProxySnapshot>>>,

    /// Windows registry state immediately after this app set the proxy
    #[cfg(target_os = "windows")]
    owned_windows_proxy: Arc<Mutex<Option<WindowsProxySnapshot>>>,

    /// helps to auto launch the app
    auto_launch: Arc<Mutex<Option<AutoLaunch>>>,

    /// serialize proxy writes, restores, and guard refreshes
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
