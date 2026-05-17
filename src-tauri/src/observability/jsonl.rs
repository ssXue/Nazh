//! JSONL 文件读写。

use std::path::Path;

use serde::{Serialize, de::DeserializeOwned};

pub(crate) async fn read_jsonl<T>(path: &Path) -> Result<Vec<T>, String>
where
    T: DeserializeOwned,
{
    if !path.exists() {
        return Ok(Vec::new());
    }

    let text = tokio::fs::read_to_string(path)
        .await
        .map_err(|error| format!("读取观测文件失败: {error}"))?;

    let mut items = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        if let Ok(item) = serde_json::from_str::<T>(line) {
            items.push(item);
        }
    }

    Ok(items)
}

pub(crate) async fn append_jsonl<T>(path: std::path::PathBuf, record: &T) -> Result<(), String>
where
    T: Serialize + Send + Sync,
{
    let line =
        serde_json::to_string(record).map_err(|error| format!("序列化观测记录失败: {error}"))?;

    tokio::task::spawn_blocking(move || -> Result<(), String> {
        use std::io::Write;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("创建观测目录失败: {error}"))?;
        }

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|error| format!("打开观测文件失败: {error}"))?;
        writeln!(file, "{line}").map_err(|error| format!("写入观测文件失败: {error}"))?;
        Ok(())
    })
    .await
    .map_err(|error| format!("写入观测记录任务失败: {error}"))?
}
