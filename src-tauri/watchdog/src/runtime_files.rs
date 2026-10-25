use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::protocol::{HEARTBEAT_FILE, QUITTING_FILE, RUNTIME_DIR};

pub enum Heartbeat {
    /// 最近一次心跳的 Unix 毫秒时间戳。
    At(u64),
    /// 本次启动后前端还没有心跳过（GUI 启动时会删除旧文件）。
    Missing,
    /// 读取失败或内容不完整（与 GUI 写入撞车），本轮无法判断。
    Unreadable,
}

/// GUI 写给看门狗的运行时文件。
pub struct RuntimeFiles {
    heartbeat: PathBuf,
    quitting: PathBuf,
}

impl RuntimeFiles {
    pub fn new(exe_dir: &Path) -> Self {
        let dir = RUNTIME_DIR
            .iter()
            .fold(exe_dir.to_path_buf(), |path, part| path.join(part));
        Self {
            heartbeat: dir.join(HEARTBEAT_FILE),
            quitting: dir.join(QUITTING_FILE),
        }
    }

    /// 最近一次前端心跳。
    ///
    /// 只读内容、不看 mtime：把空文件截断为空时 NTFS 不更新 LastWriteTime。
    pub fn heartbeat(&self) -> Heartbeat {
        match fs::read_to_string(&self.heartbeat) {
            Ok(text) => text
                .trim()
                .parse()
                .map(Heartbeat::At)
                .unwrap_or(Heartbeat::Unreadable),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Heartbeat::Missing,
            Err(_) => Heartbeat::Unreadable,
        }
    }

    /// GUI 是否在 `since_ms` 之后开始退出。
    pub fn quitting_since(&self, since_ms: u64) -> bool {
        read_timestamp_ms(&self.quitting).is_some_and(|ms| ms >= since_ms)
    }
}

fn read_timestamp_ms(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

pub fn unix_millis(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_exe_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "clash-astra-watchdog-test-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(RUNTIME_DIR.iter().fold(dir.clone(), |p, part| p.join(part))).unwrap();
        dir
    }

    #[test]
    fn heartbeat_reads_timestamp_content() {
        let dir = temp_exe_dir("heartbeat");
        let files = RuntimeFiles::new(&dir);
        assert!(matches!(files.heartbeat(), Heartbeat::Missing));

        fs::write(&files.heartbeat, "").unwrap();
        assert!(matches!(files.heartbeat(), Heartbeat::Unreadable));

        fs::write(&files.heartbeat, "1792937445123\r\n").unwrap();
        assert!(matches!(files.heartbeat(), Heartbeat::At(1792937445123)));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn quitting_marker_must_be_newer_than_child_start() {
        let dir = temp_exe_dir("quitting");
        let files = RuntimeFiles::new(&dir);
        assert!(!files.quitting_since(1_000));

        fs::write(&files.quitting, "999").unwrap();
        assert!(!files.quitting_since(1_000));

        fs::write(&files.quitting, "1000").unwrap();
        assert!(files.quitting_since(1_000));
        let _ = fs::remove_dir_all(&dir);
    }
}
