// Source: ~/claudecode/openclaudecode/src/utils/plugins/zipCache.ts
#![allow(dead_code)]

use std::fs::{self, File};
use std::io::{self, Cursor, Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use super::schemas::MarketplaceSource;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

/// Check if the plugin zip cache mode is enabled.
pub fn is_plugin_zip_cache_enabled() -> bool {
    std::env::var("CLAUDE_CODE_PLUGIN_USE_ZIP_CACHE")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false)
}

/// Get the path to the zip cache directory.
pub fn get_plugin_zip_cache_path() -> Option<String> {
    if !is_plugin_zip_cache_enabled() {
        return None;
    }
    std::env::var("CLAUDE_CODE_PLUGIN_CACHE_DIR")
        .ok()
        .map(|dir| {
            if dir.starts_with("~/") {
                dirs::home_dir()
                    .map(|h| format!("{}{}", h.display(), &dir[1..]))
                    .unwrap_or(dir)
            } else {
                dir
            }
        })
}

/// Get the path to known_marketplaces.json in the zip cache.
pub fn get_zip_cache_known_marketplaces_path()
-> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let cache_path =
        get_plugin_zip_cache_path().ok_or_else(|| "Plugin zip cache is not enabled".to_string())?;
    Ok(PathBuf::from(cache_path)
        .join("known_marketplaces.json")
        .to_string_lossy()
        .to_string())
}

/// Get the path to installed_plugins.json in the zip cache.
pub fn get_zip_cache_installed_plugins_path()
-> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let cache_path =
        get_plugin_zip_cache_path().ok_or_else(|| "Plugin zip cache is not enabled".to_string())?;
    Ok(PathBuf::from(cache_path)
        .join("installed_plugins.json")
        .to_string_lossy()
        .to_string())
}

/// Get the marketplaces directory within the zip cache.
pub fn get_zip_cache_marketplaces_dir() -> Result<String, Box<dyn std::error::Error + Send + Sync>>
{
    let cache_path =
        get_plugin_zip_cache_path().ok_or_else(|| "Plugin zip cache is not enabled".to_string())?;
    Ok(PathBuf::from(cache_path)
        .join("marketplaces")
        .to_string_lossy()
        .to_string())
}

/// Get the plugins directory within the zip cache.
pub fn get_zip_cache_plugins_dir() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let cache_path =
        get_plugin_zip_cache_path().ok_or_else(|| "Plugin zip cache is not enabled".to_string())?;
    Ok(PathBuf::from(cache_path)
        .join("plugins")
        .to_string_lossy()
        .to_string())
}

/// Session plugin cache: a temp directory on local disk.
static SESSION_PLUGIN_CACHE_PATH: once_cell::sync::Lazy<std::sync::Mutex<Option<String>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(None));

/// Get or create the session plugin cache directory.
pub async fn get_session_plugin_cache_path()
-> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    {
        let guard = SESSION_PLUGIN_CACHE_PATH.lock().unwrap();
        if let Some(ref path) = *guard {
            return Ok(path.clone());
        }
    }

    let suffix = hex::encode(rand::random::<[u8; 8]>());
    let dir = PathBuf::from(std::env::temp_dir()).join(format!("claude-plugin-session-{}", suffix));

    tokio::fs::create_dir_all(&dir).await?;

    let path_str = dir.to_string_lossy().to_string();
    {
        let mut guard = SESSION_PLUGIN_CACHE_PATH.lock().unwrap();
        *guard = Some(path_str.clone());
    }

    log::debug!("Created session plugin cache at {}", path_str);
    Ok(path_str)
}

/// Clean up the session plugin cache directory.
pub async fn cleanup_session_plugin_cache() -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    let path = {
        let mut guard = SESSION_PLUGIN_CACHE_PATH.lock().unwrap();
        guard.take()
    };
    if let Some(path) = path {
        if let Err(e) = tokio::fs::remove_dir_all(&path).await {
            log::debug!("Failed to clean up session plugin cache at {}: {}", path, e);
        } else {
            log::debug!("Cleaned up session plugin cache at {}", path);
        }
    }
    Ok(())
}

/// Write data to a file in the zip cache atomically.
pub async fn atomic_write_to_zip_cache(
    target_path: &str,
    data: &[u8],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let dir = Path::new(target_path)
        .parent()
        .ok_or_else(|| "Invalid target path".to_string())?;
    tokio::fs::create_dir_all(dir).await?;

    let file_name = Path::new(target_path)
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();

    let tmp_name = format!(
        ".{}.tmp.{}",
        file_name,
        hex::encode(rand::random::<[u8; 4]>())
    );
    let tmp_path = dir.join(&tmp_name);

    tokio::fs::write(&tmp_path, data).await?;
    tokio::fs::rename(&tmp_path, target_path).await?;

    Ok(())
}

/// Create a ZIP archive from a directory.
pub async fn create_zip_from_directory(source_dir: &Path) -> Result<Vec<u8>, String> {
    let source_path = source_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));

        collect_and_add_files(&source_path, &source_path, &mut writer)?;

        let cursor = writer.finish().map_err(|e| format!("Failed to finish zip: {}", e))?;
        let buffer = cursor.into_inner();
        log::debug!(
            "Created ZIP from {}: {} bytes",
            source_path.display(),
            buffer.len()
        );
        Ok(buffer)
    })
    .await
    .map_err(|e| format!("Join error in create_zip_from_directory: {}", e))?
}

/// Recursively collect files from a directory and add them to the ZIP writer.
/// Skips .git directories, symlinked directories, and handles symlinked files.
/// Preserves Unix mode bits (including executable bits) in the ZIP archive.
fn collect_and_add_files(
    base_dir: &Path,
    current_dir: &Path,
    writer: &mut ZipWriter<Cursor<Vec<u8>>>,
) -> Result<(), String> {
    let entries =
        fs::read_dir(current_dir).map_err(|e| format!("Failed to read directory {}: {}", current_dir.display(), e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();

        // Skip .git directories
        if name_str == ".git" {
            continue;
        }

        let full_path = entry.path();
        let relative_path = if current_dir == base_dir {
            file_name.to_string_lossy().into_owned()
        } else {
            let rel = current_dir
                .strip_prefix(base_dir)
                .map_err(|e| format!("Failed to strip prefix: {}", e))?
                .to_string_lossy();
            format!("{}/{}", rel, name_str)
        };

        let lstat = entry
            .file_type()
            .map_err(|e| format!("Failed to get file type for {}: {}", full_path.display(), e))?;

        if lstat.is_symlink() {
            // Resolve symlink target
            let target_metadata = fs::metadata(&full_path)
                .map_err(|_| format!("Broken symlink: {}", full_path.display()))?;
            if target_metadata.is_dir() {
                // Skip symlinked directories
                continue;
            }
            // Follow symlinked file — add with resolved content
            let content =
                fs::read(&full_path).map_err(|e| format!("Failed to read file {}: {}", full_path.display(), e))?;
            let mode = (target_metadata.mode() & 0xFFFF) as u32;
            add_zip_entry(writer, &relative_path, &content, mode)?;
        } else if lstat.is_dir() {
            collect_and_add_files(base_dir, &full_path, writer)?;
        } else if lstat.is_file() {
            let metadata = entry
                .metadata()
                .map_err(|e| format!("Failed to get metadata for {}: {}", full_path.display(), e))?;
            let content =
                fs::read(&full_path).map_err(|e| format!("Failed to read file {}: {}", full_path.display(), e))?;
            let mode = (metadata.mode() & 0xFFFF) as u32;
            add_zip_entry(writer, &relative_path, &content, mode)?;
        }
    }

    Ok(())
}

/// Add a single file entry to the ZIP archive with Unix mode bits preserved.
fn add_zip_entry(
    writer: &mut ZipWriter<Cursor<Vec<u8>>>,
    name: &str,
    data: &[u8],
    mode: u32,
) -> Result<(), String> {
    writer
        .start_file(name, SimpleFileOptions::default().unix_permissions(mode))
        .map_err(|e| format!("Failed to start zip entry {}: {}", name, e))?;
    writer
        .write_all(data)
        .map_err(|e| format!("Failed to write zip entry {}: {}", name, e))?;
    Ok(())
}

/// Extract a ZIP file to a target directory.
pub async fn extract_zip_to_directory(
    zip_path: &str,
    target_dir: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let zip_path = zip_path.to_string();
    let target = target_dir.to_string();
    tokio::task::spawn_blocking(move || {
        let result: std::io::Result<()> = (|| {
            fs::create_dir_all(&target)?;

            let file = File::open(&zip_path)?;
            let mut archive = ZipArchive::new(file)?;

            for i in 0..archive.len() {
                let mut entry = archive.by_index(i)?;

                if entry.is_dir() {
                    let entry_path = entry.enclosed_name().ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "Invalid zip entry name")
                    })?;
                    fs::create_dir_all(Path::new(&target).join(&entry_path))?;
                    continue;
                }

                let entry_path = entry.enclosed_name().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "Invalid zip entry name")
                })?;
                let full_path = PathBuf::from(&target).join(&entry_path);

                if let Some(parent) = full_path.parent() {
                    fs::create_dir_all(parent)?;
                }

                let mut outfile = File::create(&full_path)?;
                io::copy(&mut entry, &mut outfile)?;
                drop(outfile);

                // Restore Unix executable bits from zip entry metadata
                if let Some(mode) = entry.unix_mode() {
                    if mode & 0o111 != 0 {
                        let current = fs::metadata(&full_path)?.permissions();
                        let mut perms = current.clone();
                        let new_mode = mode & 0o777;
                        perms.set_mode(new_mode);
                        fs::set_permissions(&full_path, perms).ok();
                    }
                }
            }

            Ok(())
        })();

        result.map_err(|e| format!("Failed to extract zip: {}", e))
    })
    .await
    .map_err(|e| format!("Join error in extract_zip_to_directory: {}", e))?
    .map_err(|e| e.into())
}

/// Convert a plugin directory to a ZIP in-place.
pub async fn convert_directory_to_zip_in_place(
    dir_path: &str,
    zip_path: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let zip_data = create_zip_from_directory(Path::new(dir_path)).await?;
    atomic_write_to_zip_cache(zip_path, &zip_data).await?;
    let _ = tokio::fs::remove_dir_all(dir_path).await;
    Ok(())
}

/// Get the relative path for a marketplace JSON file within the zip cache.
pub fn get_marketplace_json_relative_path(marketplace_name: &str) -> String {
    let sanitized = marketplace_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("marketplaces/{}.json", sanitized)
}

/// Check if a marketplace source type is supported by zip cache mode.
pub fn is_marketplace_source_supported_by_zip_cache(source: &MarketplaceSource) -> bool {
    matches!(
        source,
        MarketplaceSource::Github { .. }
            | MarketplaceSource::Git { .. }
            | MarketplaceSource::Url { .. }
            | MarketplaceSource::Settings { .. }
    )
}
