use crate::config::DEFAULT_LATENCY_TEST_URL;
use crate::utils::{dirs, help};
use anyhow::Result;
use log::LevelFilter;
use serde::{Deserialize, Serialize};

pub const MIHOMO_CORE: &str = "mihomo";
pub const MIHOMO_ALPHA_CORE: &str = "mihomo-alpha";
pub const VALID_MIHOMO_CORES: [&str; 2] = [MIHOMO_CORE, MIHOMO_ALPHA_CORE];

/// 应用设置的持久化字段；保留既有 YAML 字段名以兼容用户配置。
#[derive(Default, Debug, Clone, Deserialize, Serialize)]
pub struct IVerge {
    /// 单实例唤起服务的监听端口。
    pub app_singleton_port: Option<u16>,

    /// 应用日志等级：silent、error、warn、info、debug、trace。
    pub app_log_level: Option<String>,

    // 界面语言。
    pub language: Option<String>,

    /// 主题模式：light、dark 或 system。
    pub theme_mode: Option<String>,

    /// 托盘单击执行的操作。
    pub tray_event: Option<String>,

    /// 复制代理环境变量时使用的终端语法。
    pub env_type: Option<String>,

    /// 启动后显示的页面。
    pub start_page: Option<String>,
    /// 用户启动脚本路径。
    pub startup_script: Option<String>,

    /// 是否显示流量图，默认开启。
    pub traffic_graph: Option<bool>,

    /// 是否显示 Mihomo 内存用量。
    pub enable_memory_usage: Option<bool>,

    /// 是否显示代理组图标。
    pub enable_group_icon: Option<bool>,

    /// 是否使用自定义普通托盘图标。
    pub common_tray_icon: Option<bool>,

    /// 是否使用自定义系统代理托盘图标。
    pub sysproxy_tray_icon: Option<bool>,

    /// 是否使用自定义 TUN 托盘图标。
    pub tun_tray_icon: Option<bool>,

    /// 是否启用 TUN。
    pub enable_tun_mode: Option<bool>,

    /// 是否由 Windows 服务管理内核。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_service_mode: Option<bool>,

    /// 是否随系统自动启动。
    pub enable_auto_launch: Option<bool>,

    /// 启动时是否隐藏窗口。
    pub enable_silent_start: Option<bool>,

    /// 是否使用透明无边框窗口；默认使用带系统边框的窗口以降低 WebView2 挂起风险。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_custom_frameless_window: Option<bool>,

    /// 是否接管系统代理。
    pub enable_system_proxy: Option<bool>,

    /// 是否启用用户配置的代理守卫。
    pub enable_proxy_guard: Option<bool>,

    /// 系统代理绕过列表。
    pub system_proxy_bypass: Option<String>,

    /// 代理守卫检查间隔，单位为秒。
    pub proxy_guard_duration: Option<u64>,

    /// 用户主题配置。
    pub theme_setting: Option<IVergeTheme>,

    /// 外部 Web 控制界面列表。
    pub web_ui_list: Option<Vec<String>>,

    /// 使用的 Mihomo 内核标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clash_core: Option<String>,

    /// 快捷键列表，格式为「操作名,快捷键」。
    pub hotkeys: Option<Vec<String>>,

    /// 切换代理时自动关闭连接
    pub auto_close_connection: Option<bool>,

    /// 默认的延迟测试连接
    pub default_latency_test: Option<String>,

    /// 默认的延迟测试超时时间
    pub default_latency_timeout: Option<i32>,

    /// 是否使用内部的脚本支持，默认为真
    pub enable_builtin_enhanced: Option<bool>,

    /// proxy 页面布局 列数
    pub proxy_layout_column: Option<i32>,

    /// 测试网站列表
    pub test_list: Option<Vec<IVergeTestItem>>,

    /// 日志清理
    /// 0: 不清理; 1: 7天; 2: 30天; 3: 90天
    pub auto_log_clean: Option<i32>,

    /// 窗口逻辑尺寸与位置：宽、高、横坐标、纵坐标。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_size_position: Option<Vec<f64>>,

    /// 窗口是否最大化。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_is_maximized: Option<bool>,

    /// 是否锁定窗口尺寸。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_size_locked: Option<bool>,

    /// 是否启用随机端口
    pub enable_random_port: Option<bool>,

    /// 持久化的混合代理端口，与内核端口在同一次操作中提交。
    pub verge_mixed_port: Option<u16>,

    pub verge_socks_port: Option<u16>,

    pub verge_port: Option<u16>,
}

#[derive(Default, Debug, Clone, Deserialize, Serialize)]
pub struct IVergeTestItem {
    pub uid: Option<String>,
    pub name: Option<String>,
    pub icon: Option<String>,
    pub url: Option<String>,
}

#[derive(Default, Debug, Clone, Deserialize, Serialize)]
pub struct IVergeTheme {
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,
    pub primary_text: Option<String>,
    pub secondary_text: Option<String>,

    pub info_color: Option<String>,
    pub error_color: Option<String>,
    pub warning_color: Option<String>,
    pub success_color: Option<String>,

    pub css_injection: Option<String>,
}

impl IVerge {
    pub fn new() -> Self {
        let path = match dirs::verge_path() {
            Ok(path) => path,
            Err(err) => {
                log::error!(target: "app", "{err}");
                return Self::template();
            }
        };

        match help::read_yaml::<IVerge>(&path) {
            Ok(mut config) => {
                let mut changed = config.normalize_auto_log_clean();
                changed |= config.normalize_default_latency_test();

                if changed {
                    let _ = config.save_file();
                }
                config
            }
            Err(err) => {
                log::error!(target: "app", "{err}");
                if path.exists() {
                    if let Err(error) = help::quarantine_corrupt_file(&path) {
                        crate::infrastructure::config_repository::block_writes(format!(
                            "{error:#}"
                        ));
                    }
                }
                Self::template()
            }
        }
    }

    pub fn template() -> Self {
        Self {
            clash_core: Some(MIHOMO_CORE.into()),
            language: Some("zh".into()),
            theme_mode: Some("system".into()),
            env_type: Some("powershell".into()),
            start_page: Some("/".into()),
            traffic_graph: Some(true),
            enable_memory_usage: Some(true),
            enable_group_icon: Some(true),
            common_tray_icon: Some(false),
            sysproxy_tray_icon: Some(false),
            tun_tray_icon: Some(false),
            enable_auto_launch: Some(false),
            enable_silent_start: Some(false),
            enable_system_proxy: Some(false),
            enable_random_port: Some(false),
            verge_mixed_port: Some(7897),
            verge_socks_port: Some(7898),
            verge_port: Some(7899),
            enable_proxy_guard: Some(false),
            proxy_guard_duration: Some(30),
            auto_close_connection: Some(true),
            enable_builtin_enhanced: Some(true),
            auto_log_clean: Some(1),
            default_latency_test: Some(DEFAULT_LATENCY_TEST_URL.into()),
            ..Self::default()
        }
    }

    pub fn normalize_auto_log_clean(&mut self) -> bool {
        let normalized = match self.auto_log_clean {
            Some(0 | 1 | 2 | 3) => self.auto_log_clean,
            _ => Some(1),
        };

        if self.auto_log_clean != normalized {
            self.auto_log_clean = normalized;
            return true;
        }

        false
    }

    pub fn normalize_default_latency_test(&mut self) -> bool {
        let normalized = match self.default_latency_test.as_deref() {
            Some(url) if url.trim().is_empty() => Some(DEFAULT_LATENCY_TEST_URL.to_string()),
            Some(_) => self.default_latency_test.clone(),
            None => Some(DEFAULT_LATENCY_TEST_URL.to_string()),
        };

        if self.default_latency_test != normalized {
            self.default_latency_test = normalized;
            return true;
        }

        false
    }

    /// 启动迁移时保存设置；运行中的修改统一经应用协调器提交。
    pub fn save_file(&self) -> Result<()> {
        help::save_yaml(&dirs::verge_path()?, &self, Some("# Clash Astra Config"))
    }

    /// 将请求中的已指定字段合入候选设置；此方法不执行 I/O。
    pub fn patch_config(&mut self, patch: IVerge) {
        macro_rules! patch {
            ($key: tt) => {
                if patch.$key.is_some() {
                    self.$key = patch.$key;
                }
            };
        }

        patch!(app_log_level);
        patch!(language);
        patch!(theme_mode);
        patch!(tray_event);
        patch!(env_type);
        patch!(start_page);
        patch!(startup_script);
        patch!(traffic_graph);
        patch!(enable_memory_usage);
        patch!(enable_group_icon);
        patch!(common_tray_icon);
        patch!(sysproxy_tray_icon);
        patch!(tun_tray_icon);

        patch!(enable_tun_mode);
        patch!(enable_service_mode);
        patch!(enable_auto_launch);
        patch!(enable_silent_start);
        patch!(enable_custom_frameless_window);
        patch!(enable_random_port);
        patch!(verge_mixed_port);
        patch!(verge_socks_port);
        patch!(verge_port);
        patch!(enable_system_proxy);
        patch!(enable_proxy_guard);
        patch!(system_proxy_bypass);
        patch!(proxy_guard_duration);

        patch!(theme_setting);
        patch!(web_ui_list);
        patch!(clash_core);
        patch!(hotkeys);

        patch!(auto_close_connection);
        patch!(default_latency_test);
        patch!(default_latency_timeout);
        patch!(enable_builtin_enhanced);
        patch!(proxy_layout_column);
        patch!(test_list);
        patch!(auto_log_clean);
        patch!(window_size_position);
        patch!(window_is_maximized);
        patch!(window_size_locked);
    }

    /// 在初始化前尝试拿到单例端口的值
    pub fn get_singleton_port() -> u16 {
        #[cfg(not(feature = "verge-dev"))]
        const SERVER_PORT: u16 = 33331;
        #[cfg(feature = "verge-dev")]
        const SERVER_PORT: u16 = 11233;

        match dirs::verge_path().and_then(|path| help::read_yaml::<IVerge>(&path)) {
            Ok(config) => config.app_singleton_port.unwrap_or(SERVER_PORT),
            Err(_) => SERVER_PORT, // 这里就不log错误了
        }
    }

    /// 获取日志等级
    pub fn get_log_level(&self) -> LevelFilter {
        if let Some(level) = self.app_log_level.as_ref() {
            match level.to_lowercase().as_str() {
                "silent" => LevelFilter::Off,
                "error" => LevelFilter::Error,
                "warn" => LevelFilter::Warn,
                "info" => LevelFilter::Info,
                "debug" => LevelFilter::Debug,
                "trace" => LevelFilter::Trace,
                _ => LevelFilter::Info,
            }
        } else {
            LevelFilter::Info
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_uses_default_latency_test_url() {
        assert_eq!(
            IVerge::template().default_latency_test.as_deref(),
            Some(DEFAULT_LATENCY_TEST_URL)
        );
    }

    #[test]
    fn template_uses_mihomo_core_identifier() {
        assert_eq!(IVerge::template().clash_core.as_deref(), Some(MIHOMO_CORE));
    }

    #[test]
    fn normalize_default_latency_test_fills_none() {
        let mut config = IVerge {
            default_latency_test: None,
            ..IVerge::default()
        };

        assert!(config.normalize_default_latency_test());
        assert_eq!(
            config.default_latency_test.as_deref(),
            Some(DEFAULT_LATENCY_TEST_URL)
        );
    }

    #[test]
    fn normalize_default_latency_test_fills_empty_string() {
        let mut config = IVerge {
            default_latency_test: Some("   ".into()),
            ..IVerge::default()
        };

        assert!(config.normalize_default_latency_test());
        assert_eq!(
            config.default_latency_test.as_deref(),
            Some(DEFAULT_LATENCY_TEST_URL)
        );
    }

    #[test]
    fn normalize_default_latency_test_preserves_custom_url() {
        let custom_url = "https://example.com/healthz";
        let mut config = IVerge {
            default_latency_test: Some(custom_url.into()),
            ..IVerge::default()
        };

        assert!(!config.normalize_default_latency_test());
        assert_eq!(config.default_latency_test.as_deref(), Some(custom_url));
    }

    #[test]
    fn normalize_default_latency_test_keeps_current_default_without_changes() {
        let mut config = IVerge {
            default_latency_test: Some(DEFAULT_LATENCY_TEST_URL.into()),
            ..IVerge::default()
        };

        assert!(!config.normalize_default_latency_test());
        assert_eq!(
            config.default_latency_test.as_deref(),
            Some(DEFAULT_LATENCY_TEST_URL)
        );
    }
}
