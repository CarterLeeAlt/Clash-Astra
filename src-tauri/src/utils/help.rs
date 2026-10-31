use crate::utils::dirs;
use anyhow::{anyhow, bail, Context, Result};
use nanoid::nanoid;
use serde::{de::DeserializeOwned, Serialize};
use serde_yaml::{Mapping, Value};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

/// 读取 YAML 并反序列化为指定结构。
pub fn read_yaml<T: DeserializeOwned>(path: &PathBuf) -> Result<T> {
    if !path.exists() {
        bail!("file not found \"{}\"", path.display());
    }

    let yaml_str = fs::read_to_string(path)
        .with_context(|| format!("failed to read the file \"{}\"", path.display()))?;

    serde_yaml::from_str::<T>(&yaml_str).with_context(|| {
        format!(
            "failed to read the file with yaml format \"{}\"",
            path.display()
        )
    })
}

/// 读取 YAML 映射并应用合并键，拒绝非映射根节点。
pub fn read_merge_mapping(path: &PathBuf) -> Result<Mapping> {
    let mut val: Value = read_yaml(path)?;
    val.apply_merge()
        .with_context(|| format!("failed to apply merge \"{}\"", path.display()))?;

    Ok(val
        .as_mapping()
        .ok_or(anyhow!(
            "failed to transform to yaml mapping \"{}\"",
            path.display()
        ))?
        .to_owned())
}

/// 保留损坏的配置文件，避免下一次保存直接覆盖，便于事后排查。
pub fn quarantine_corrupt_file(path: &PathBuf) -> Result<()> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config");
    let backup = path.with_file_name(format!("{file_name}.corrupt-{timestamp}"));
    fs::rename(path, &backup).with_context(|| {
        format!(
            "failed to preserve corrupt configuration {}",
            path.display()
        )
    })?;
    log::warn!(target: "app", "corrupt config preserved at {}", backup.display());
    Ok(())
}

/// 将结构序列化为 YAML 后原子写入；prefix 用于文件头说明。
pub fn save_yaml<T: Serialize>(path: &PathBuf, data: &T, prefix: Option<&str>) -> Result<()> {
    let data_str = serde_yaml::to_string(data)?;

    let yaml_str = match prefix {
        Some(prefix) => format!("{prefix}\n\n{data_str}"),
        None => data_str,
    };

    let path_str = path.as_os_str().to_string_lossy().to_string();
    write_file_atomic(path, yaml_str.as_bytes())
        .with_context(|| format!("failed to save file \"{path_str}\""))
}

pub fn write_file_atomic(path: &PathBuf, data: &[u8]) -> Result<()> {
    if path.exists() && !path.is_file() {
        bail!(
            "refuse to replace a non-file configuration path: {}",
            path.display()
        );
    }
    let parent = path
        .parent()
        .ok_or(anyhow!("failed to get parent dir for {}", path.display()))?;

    if !parent.exists() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent dir '{}'", parent.display()))?;
    }

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_name = format!(".{}.{}.tmp", nanoid!(6, &ALPHABET), nanos);
    let tmp_path = parent.join(tmp_name);

    let write_result = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&tmp_path)
            .with_context(|| format!("failed to create temp file '{}'", tmp_path.display()))?;
        file.write_all(data)
            .with_context(|| format!("failed to write temp file '{}'", tmp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync temp file '{}'", tmp_path.display()))?;
        Ok(())
    })();

    if let Err(err) = write_result {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }

    match fs::rename(&tmp_path, path) {
        Ok(_) => Ok(()),
        Err(err) => {
            // 同目录替换失败时保留原文件，不能通过搬走原文件制造缺失窗口；
            // 恢复记录自身也使用本方法，必须具备相同的完整性保证。
            let _ = fs::remove_file(&tmp_path);
            Err(err).with_context(|| {
                format!(
                    "failed to replace file '{}' with temp file '{}'",
                    path.display(),
                    tmp_path.display()
                )
            })
        }
    }
}
const ALPHABET: [char; 62] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i',
    'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z', 'A', 'B',
    'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U',
    'V', 'W', 'X', 'Y', 'Z',
];

/// 生成带类型前缀的条目标识。
pub fn get_uid(prefix: &str) -> String {
    let id = nanoid!(11, &ALPHABET);
    format!("{prefix}{id}")
}

/// 解析分号分隔的键值参数，例如 xxx=123123 中的数值。
pub fn parse_str<T: FromStr>(target: &str, key: &str) -> Option<T> {
    target.split(';').map(str::trim).find_map(|s| {
        let mut parts = s.splitn(2, '=');
        match (parts.next(), parts.next()) {
            (Some(k), Some(v)) if k == key => v.parse::<T>().ok(),
            _ => None,
        }
    })
}

/// 取 URL 最后的路径段并解码；缺少该段时返回 None。
pub fn get_last_part_and_decode(url: &str) -> Option<String> {
    let path = url.split('?').next().unwrap_or(""); // Splits URL and takes the path part
    let segments: Vec<&str> = path.split('/').collect();
    let last_segment = segments.last()?;

    Some(
        percent_encoding::percent_decode_str(last_segment)
            .decode_utf8_lossy()
            .to_string(),
    )
}

pub fn resolve_profile_path(file: &str) -> Result<PathBuf> {
    let file = file.trim();
    if file.is_empty() {
        bail!("Invalid profile file name");
    }

    if file == "." || file == ".." {
        bail!("Invalid profile file name");
    }

    let candidate = std::path::Path::new(file);
    if candidate.is_absolute() {
        bail!("Invalid profile file name");
    }

    if candidate.components().count() != 1 {
        bail!("Invalid profile file name");
    }

    if file.contains(['/', '\\']) || file.contains(':') {
        bail!("Invalid profile file name");
    }

    let base = dirs::app_profiles_dir()?;
    Ok(base.join(file))
}
#[macro_export]
macro_rules! error {
    ($result: expr) => {
        log::error!(target: "app", "{}", $result);
    };
}

#[macro_export]
macro_rules! log_err {
    ($result: expr) => {
        if let Err(err) = $result {
            log::error!(target: "app", "{err}");
        }
    };

    ($result: expr, $err_str: expr) => {
        if let Err(_) = $result {
            log::error!(target: "app", "{}", $err_str);
        }
    };
}

#[macro_export]
macro_rules! trace_err {
    ($result: expr, $err_str: expr) => {
        if let Err(err) = $result {
            log::trace!(target: "app", "{}, err {}", $err_str, err);
        }
    }
}

/// IPC 边界保留错误链及事务阶段，前端按结构化字段呈现错误。
#[macro_export]
macro_rules! wrap_err {
    ($stat: expr) => {
        match $stat {
            Ok(a) => Ok(a),
            Err(err) => {
                let error = anyhow::Error::from(err);
                log::error!(target: "app", "{error:#}");
                Err(crate::commands::CommandError::from(error))
            }
        }
    };
}

/// 将字面错误转换为统一的 IPC 错误。
#[macro_export]
macro_rules! ret_err {
    ($str: expr) => {
        return Err($str.into())
    };
}

#[test]
fn test_parse_value() {
    let test_1 = "upload=111; download=2222; total=3333; expire=444";
    let test_2 = "attachment; filename=Clash.yaml";

    assert_eq!(parse_str::<usize>(test_1, "upload").unwrap(), 111);
    assert_eq!(parse_str::<usize>(test_1, "download").unwrap(), 2222);
    assert_eq!(parse_str::<usize>(test_1, "total").unwrap(), 3333);
    assert_eq!(parse_str::<usize>(test_1, "expire").unwrap(), 444);
    assert_eq!(
        parse_str::<String>(test_2, "filename").unwrap(),
        format!("Clash.yaml")
    );

    assert_eq!(parse_str::<usize>(test_1, "aaa"), None);
    assert_eq!(parse_str::<usize>(test_1, "upload1"), None);
    assert_eq!(parse_str::<usize>(test_1, "expire1"), None);
    assert_eq!(parse_str::<usize>(test_2, "attachment"), None);
}
