use crate::utils::{help, resolve::VERSION, tmpl};
use anyhow::{bail, Context, Result};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::Duration;
use sysproxy::Sysproxy;

use super::Config;

pub const GLOBAL_SCRIPT_UID: &str = "__global_script__";
pub const GLOBAL_SCRIPT_FILE: &str = "global-script.js";
pub const GLOBAL_SCRIPT_NAME: &str = "Global Script";
pub const GLOBAL_SCRIPT_DESC: &str = "Global Script";

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PrfItem {
    pub uid: Option<String>,

    /// 条目类型：remote、local、script 或 merge。
    #[serde(rename = "type")]
    pub itype: Option<String>,

    /// 显示名称。
    pub name: Option<String>,

    /// 配置目录内的相对文件名。
    pub file: Option<String>,

    /// 用户备注。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,

    /// 远程订阅来源地址。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// 各代理组的持久化节点选择。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<Vec<PrfSelected>>,

    /// 订阅返回的流量和到期信息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<PrfExtra>,

    /// 最近更新时间，Unix 秒。
    pub updated: Option<usize>,

    /// 下载与自动更新选项。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option: Option<PrfOption>,

    /// 尚未落盘的文件内容，不写入订阅索引。
    #[serde(skip)]
    pub file_data: Option<String>,
}

#[derive(Default, Debug, Clone, Deserialize, Serialize)]
pub struct PrfSelected {
    pub name: Option<String>,
    pub now: Option<String>,
}

#[derive(Default, Debug, Clone, Copy, Deserialize, Serialize)]
pub struct PrfExtra {
    pub upload: u64,
    pub download: u64,
    pub total: u64,
    pub expire: u64,
}

#[derive(Default, Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PrfOption {
    /// 远程订阅请求的 User-Agent。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,

    /// 下载远程订阅时使用当前系统代理。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with_proxy: Option<bool>,

    /// 下载远程订阅时使用本程序混合代理端口。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_proxy: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_interval: Option<u64>,

    /// 是否跳过远程订阅的证书校验，默认关闭。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub danger_accept_invalid_certs: Option<bool>,
}

impl PrfOption {
    pub fn merge(one: Option<Self>, other: Option<Self>) -> Option<Self> {
        match (one, other) {
            (Some(mut a), Some(b)) => {
                a.user_agent = b.user_agent.or(a.user_agent);
                a.with_proxy = b.with_proxy.or(a.with_proxy);
                a.self_proxy = b.self_proxy.or(a.self_proxy);
                a.danger_accept_invalid_certs = b
                    .danger_accept_invalid_certs
                    .or(a.danger_accept_invalid_certs);
                a.update_interval = b.update_interval.or(a.update_interval);
                Some(a)
            }
            t => t.0.or(t.1),
        }
    }
}

impl PrfItem {
    /// 根据创建请求生成条目；请求必须指定条目类型。
    pub async fn from(item: PrfItem, file_data: Option<String>) -> Result<PrfItem> {
        if item.itype.is_none() {
            bail!("type should not be null");
        }

        match item.itype.unwrap().as_str() {
            "remote" => {
                if item.url.is_none() {
                    bail!("url should not be null");
                }
                let url = item.url.as_ref().unwrap().as_str();
                let name = item.name;
                let desc = item.desc;
                PrfItem::from_url(url, name, desc, item.option).await
            }
            "local" => {
                let name = item.name.unwrap_or("Local File".into());
                let desc = item.desc.unwrap_or("".into());
                let mut created = PrfItem::from_local(name, desc, file_data)?;
                created.option = item.option;
                Ok(created)
            }
            "merge" => {
                let name = item.name.unwrap_or("Merge".into());
                let desc = item.desc.unwrap_or("".into());
                PrfItem::from_merge(name, desc)
            }
            "script" => {
                let name = item.name.unwrap_or("Script".into());
                let desc = item.desc.unwrap_or("".into());
                PrfItem::from_script(name, desc)
            }
            typ => bail!("invalid profile item type \"{typ}\""),
        }
    }

    /// 构造本地订阅，文件内容由后续事务写入。
    pub fn from_local(name: String, desc: String, file_data: Option<String>) -> Result<PrfItem> {
        let uid = help::get_uid("l");
        let file = format!("{uid}.yaml");

        Ok(PrfItem {
            uid: Some(uid),
            itype: Some("local".into()),
            name: Some(name),
            desc: Some(desc),
            file: Some(file),
            url: None,
            selected: None,
            extra: None,
            option: None,
            updated: Some(chrono::Local::now().timestamp() as usize),
            file_data: Some(file_data.unwrap_or(tmpl::ITEM_LOCAL.into())),
        })
    }

    /// 下载并校验远程订阅，返回尚未落盘的内容和元数据。
    pub async fn from_url(
        url: &str,
        name: Option<String>,
        desc: Option<String>,
        option: Option<PrfOption>,
    ) -> Result<PrfItem> {
        crate::application::budget::step(Duration::from_secs(40), async {
            for attempt in 0..3 {
                match Self::from_url_once(url, name.clone(), desc.clone(), option.clone()).await {
                    Ok(item) => return Ok(item),
                    Err(error) if attempt < 2 && crate::application::budget::retryable(&error) => {
                        tokio::time::sleep(Duration::from_millis(250 * (attempt + 1))).await;
                    }
                    Err(error) => return Err(error),
                }
            }
            unreachable!()
        })
        .await
    }

    async fn from_url_once(
        url: &str,
        name: Option<String>,
        desc: Option<String>,
        option: Option<PrfOption>,
    ) -> Result<PrfItem> {
        let requested_option = option.clone();
        let opt_ref = option.as_ref();
        let with_proxy = opt_ref.map_or(false, |o| o.with_proxy.unwrap_or(false));
        let self_proxy = opt_ref.map_or(false, |o| o.self_proxy.unwrap_or(false));
        anyhow::ensure!(
            !(with_proxy && self_proxy),
            "choose either application proxy or system proxy, not both"
        );
        let accept_invalid_certs =
            opt_ref.map_or(false, |o| o.danger_accept_invalid_certs.unwrap_or(false));
        let user_agent = opt_ref.and_then(|o| o.user_agent.clone());
        let update_interval = opt_ref.and_then(|o| o.update_interval);

        let mut builder = reqwest::ClientBuilder::new()
            .use_rustls_tls()
            .no_proxy()
            .connect_timeout(Duration::from_secs(8))
            .timeout(crate::application::budget::remaining(Duration::from_secs(
                15,
            ))?);

        // 使用软件自己的代理
        if self_proxy {
            let port = Config::clash_settings()
                .clash_mixed_port
                .unwrap_or(Config::clash().get_mixed_port());

            let proxy_scheme = format!("http://127.0.0.1:{port}");

            if let Ok(proxy) = reqwest::Proxy::http(&proxy_scheme) {
                builder = builder.proxy(proxy);
            }
            if let Ok(proxy) = reqwest::Proxy::https(&proxy_scheme) {
                builder = builder.proxy(proxy);
            }
            if let Ok(proxy) = reqwest::Proxy::all(&proxy_scheme) {
                builder = builder.proxy(proxy);
            }
        }
        // 使用系统代理
        else if with_proxy {
            if let Ok(p @ Sysproxy { enable: true, .. }) = Sysproxy::get_system_proxy() {
                let proxy_scheme = format!("http://{}:{}", p.host, p.port);

                if let Ok(proxy) = reqwest::Proxy::http(&proxy_scheme) {
                    builder = builder.proxy(proxy);
                }
                if let Ok(proxy) = reqwest::Proxy::https(&proxy_scheme) {
                    builder = builder.proxy(proxy);
                }
                if let Ok(proxy) = reqwest::Proxy::all(&proxy_scheme) {
                    builder = builder.proxy(proxy);
                }
            } else {
                bail!("system proxy download was requested, but the system proxy is unavailable or disabled");
            }
        }

        let version = match VERSION.get() {
            Some(v) => format!(
                "clash-astra/v{} mihomo/{}",
                v,
                crate::core::mihomo::lock().version
            ),
            None => format!(
                "clash-astra/unknown mihomo/{}",
                crate::core::mihomo::lock().version
            ),
        };

        builder = builder.danger_accept_invalid_certs(accept_invalid_certs);
        builder = builder.user_agent(user_agent.unwrap_or(version));

        let mut resp = builder
            .build()?
            .get(url)
            .send()
            .await
            .map_err(|error| error.without_url())
            .context("failed to fetch remote profile (request limit 15s, connect limit 8s)")?;

        let status_code = resp.status();
        if !StatusCode::is_success(&status_code) {
            if let Err(error) = resp.error_for_status_ref() {
                return Err(error.without_url()).context("remote subscription HTTP rejection");
            }
            bail!("remote subscription returned unexpected status {status_code}");
        }

        let header = resp.headers();
        if let Some(version) = header.get("x-mihomo-version") {
            anyhow::ensure!(
                version.to_str().ok() == Some(crate::core::mihomo::lock().version.as_str()),
                "subscription response targets a different Mihomo version"
            );
        }
        if let Some(contract) = header.get("x-sublink-contract") {
            anyhow::ensure!(
                contract.to_str().ok() == Some(crate::core::mihomo::lock().contract.as_str()),
                "subscription response uses a different generation contract"
            );
        }

        // 解析订阅响应中的流量与到期信息。
        let extra = match header.get("Subscription-Userinfo") {
            Some(value) => {
                let sub_info = value.to_str().unwrap_or("");
                Some(PrfExtra {
                    upload: help::parse_str(sub_info, "upload").unwrap_or(0),
                    download: help::parse_str(sub_info, "download").unwrap_or(0),
                    total: help::parse_str(sub_info, "total").unwrap_or(0),
                    expire: help::parse_str(sub_info, "expire").unwrap_or(0),
                })
            }
            None => None,
        };

        // 按标准扩展参数优先的顺序解析响应文件名。
        let filename = match header.get("Content-Disposition") {
            Some(value) => {
                let filename = format!("{value:?}");
                let filename = filename.trim_matches('"');
                match help::parse_str::<String>(filename, "filename*") {
                    Some(filename) => {
                        let iter = percent_encoding::percent_decode(filename.as_bytes());
                        let filename = iter.decode_utf8().unwrap_or_default();
                        filename.split("''").last().map(|s| s.to_string())
                    }
                    None => match help::parse_str::<String>(filename, "filename") {
                        Some(filename) => {
                            let filename = filename.trim_matches('"');
                            Some(filename.to_string())
                        }
                        None => None,
                    },
                }
            }
            None => Some(
                crate::utils::help::get_last_part_and_decode(url).unwrap_or("Remote File".into()),
            ),
        };
        let option = match update_interval {
            Some(val) => Some(PrfOption {
                update_interval: Some(val),
                ..PrfOption::default()
            }),
            None => match header.get("profile-update-interval") {
                Some(value) => match value.to_str().unwrap_or("").parse::<u64>() {
                    Ok(val) => Some(PrfOption {
                        update_interval: Some(
                            val.checked_mul(60)
                                .context("subscription update interval overflow")?,
                        ), // 小时转换为分钟。
                        ..PrfOption::default()
                    }),
                    Err(_) => None,
                },
                None => None,
            },
        };

        let option = PrfOption::merge(option, requested_option);
        let uid = help::get_uid("r");
        let file = format!("{uid}.yaml");
        let name = name.unwrap_or(filename.unwrap_or("Remote File".into()));
        const MAX_PROFILE_BYTES: usize = 16 * 1024 * 1024;
        anyhow::ensure!(
            resp.content_length().unwrap_or(0) <= MAX_PROFILE_BYTES as u64,
            "subscription exceeds 16 MiB"
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|error| error.without_url())
            .context("failed to download subscription body within request deadline")?
        {
            anyhow::ensure!(
                bytes.len() + chunk.len() <= MAX_PROFILE_BYTES,
                "subscription exceeds 16 MiB"
            );
            bytes.extend_from_slice(&chunk);
        }
        let data = String::from_utf8(bytes).context("subscription must be UTF-8")?;

        // 去除 UTF-8 BOM，再交给 YAML 解析器。
        let data = data.trim_start_matches('\u{feff}');

        // 下载阶段先检查结构，激活时再由 Mihomo 执行完整配置校验。
        crate::domain::network::parse_remote_profile(data)?;

        Ok(PrfItem {
            uid: Some(uid),
            itype: Some("remote".into()),
            name: Some(name),
            desc,
            file: Some(file),
            url: Some(url.into()),
            selected: None,
            extra,
            option,
            updated: Some(chrono::Local::now().timestamp() as usize),
            file_data: Some(data.into()),
        })
    }

    /// 构造合并增强项。
    pub fn from_merge(name: String, desc: String) -> Result<PrfItem> {
        let uid = help::get_uid("m");
        let file = format!("{uid}.yaml");

        Ok(PrfItem {
            uid: Some(uid),
            itype: Some("merge".into()),
            name: Some(name),
            desc: Some(desc),
            file: Some(file),
            url: None,
            selected: None,
            extra: None,
            option: None,
            updated: Some(chrono::Local::now().timestamp() as usize),
            file_data: Some(tmpl::ITEM_MERGE.into()),
        })
    }

    /// 构造身份固定的全局脚本条目。
    pub fn global_script() -> PrfItem {
        PrfItem {
            uid: Some(GLOBAL_SCRIPT_UID.into()),
            itype: Some("script".into()),
            name: Some(GLOBAL_SCRIPT_NAME.into()),
            desc: Some(GLOBAL_SCRIPT_DESC.into()),
            file: Some(GLOBAL_SCRIPT_FILE.into()),
            url: None,
            selected: None,
            extra: None,
            option: None,
            updated: Some(chrono::Local::now().timestamp() as usize),
            file_data: None,
        }
    }

    pub fn is_global_script(&self) -> bool {
        self.uid.as_deref() == Some(GLOBAL_SCRIPT_UID) && self.itype.as_deref() == Some("script")
    }

    /// 构造由 QuickJS 执行的脚本增强项。
    pub fn from_script(name: String, desc: String) -> Result<PrfItem> {
        let uid = help::get_uid("s");
        let file = format!("{uid}.js"); // js ext

        Ok(PrfItem {
            uid: Some(uid),
            itype: Some("script".into()),
            name: Some(name),
            desc: Some(desc),
            file: Some(file),
            url: None,
            selected: None,
            extra: None,
            option: None,
            updated: Some(chrono::Local::now().timestamp() as usize),
            file_data: Some(tmpl::ITEM_SCRIPT.into()),
        })
    }

    /// 读取条目关联文件，文件名必须位于订阅目录内。
    pub fn read_file(&self) -> Result<String> {
        if self.file.is_none() {
            bail!("could not find the file");
        }

        let file = self.file.clone().unwrap();
        let path = help::resolve_profile_path(&file)?;
        fs::read_to_string(path).context("failed to read the file")
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn subscription_retries_transient_http_errors_but_not_authentication_errors() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        use warp::Filter;
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let route = warp::path::full().map(move |path: warp::path::FullPath| {
            let attempt = count.fetch_add(1, Ordering::SeqCst);
            let (status, body) = if path.as_str() == "/forbidden" { (401, "forbidden") }
                else if attempt == 0 { (503, "temporary") }
                else { (200, "proxies:\n  - {name: Synthetic, type: socks5, server: 127.0.0.1, port: 9}\nproxy-groups:\n  - {name: Main, type: select, proxies: [Synthetic]}\nrules:\n  - MATCH,Main\n") };
            warp::http::Response::builder().status(status).body(body).unwrap()
        });
        let (address, server) = warp::serve(route).bind_ephemeral(([127, 0, 0, 1], 0));
        let task = tokio::spawn(server);
        let item =
            super::PrfItem::from_url(&format!("http://{address}/subscription"), None, None, None)
                .await
                .unwrap();
        assert!(item.file_data.unwrap().contains("Synthetic"));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(
            super::PrfItem::from_url(&format!("http://{address}/forbidden"), None, None, None)
                .await
                .is_err()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        task.abort();
    }
    use super::*;

    #[test]
    fn uid_script_is_not_global_script() {
        let item = PrfItem {
            uid: Some("Script".into()),
            itype: Some("script".into()),
            ..PrfItem::default()
        };

        assert!(!item.is_global_script());
    }
}
