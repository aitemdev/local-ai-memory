use crate::paths::data_paths;
use anyhow::{anyhow, Result};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

pub fn export(target: &Path, base: Option<PathBuf>) -> Result<serde_json::Value> {
    let paths = data_paths(base);
    if !paths.base.exists() {
        return Err(anyhow!("no .memoria at {}", paths.base.display()));
    }
    let parent_target = target
        .parent()
        .ok_or_else(|| anyhow!("target has no parent"))?;
    fs::create_dir_all(parent_target)?;

    let parent_dir = paths
        .base
        .parent()
        .ok_or_else(|| anyhow!("source has no parent"))?;
    let folder_name = paths
        .base
        .file_name()
        .ok_or_else(|| anyhow!("source has no name"))?;

    let status = Command::new("tar")
        .arg("-czf")
        .arg(target)
        .arg("-C")
        .arg(parent_dir)
        .arg(folder_name)
        .status()?;
    if !status.success() {
        return Err(anyhow!("tar exited with {:?}", status.code()));
    }
    let bytes = fs::metadata(target)?.len();
    Ok(serde_json::json!({
        "ok": true,
        "archive": target.to_string_lossy(),
        "bytes": bytes,
        "source": paths.base.to_string_lossy(),
    }))
}

pub fn import(archive: &Path, base: Option<PathBuf>, force: bool) -> Result<serde_json::Value> {
    let paths = data_paths(base);
    if paths.base.exists() {
        let has_data = fs::read_dir(&paths.base)?.any(|_| true);
        if has_data && !force {
            return Err(anyhow!(
                ".memoria already exists at {}; pass --force to overwrite",
                paths.base.display()
            ));
        }
        if force {
            let _ = fs::remove_dir_all(&paths.base);
        }
    }
    let parent_dir = paths
        .base
        .parent()
        .ok_or_else(|| anyhow!("destination has no parent"))?;
    fs::create_dir_all(parent_dir)?;

    let status = Command::new("tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(parent_dir)
        .status()?;
    if !status.success() {
        return Err(anyhow!("tar exited with {:?}", status.code()));
    }
    Ok(serde_json::json!({
        "ok": true,
        "archive": archive.to_string_lossy(),
        "destination": paths.base.to_string_lossy(),
    }))
}

#[allow(dead_code)]
fn copy_file_streamed(src: &Path, dst: &Path) -> Result<()> {
    let mut input = fs::File::open(src)?;
    let mut output = fs::File::create(dst)?;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        output.write_all(&buf[..n])?;
    }
    Ok(())
}
