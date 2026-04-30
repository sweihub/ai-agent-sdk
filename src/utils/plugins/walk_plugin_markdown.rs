// Source: ~/claudecode/openclaudecode/src/utils/plugins/walkPluginMarkdown.ts
#![allow(dead_code)]

use std::path::Path;

/// Options for walking a plugin directory.
#[derive(Debug, Clone, Default)]
pub struct WalkPluginMarkdownOpts {
    pub stop_at_skill_dir: Option<bool>,
    pub log_label: Option<String>,
}

/// Error type for walk_plugin_markdown operations.
#[derive(Debug)]
pub struct WalkPluginMarkdownError {
    pub message: String,
    pub path: String,
}

impl std::fmt::Display for WalkPluginMarkdownError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "walk_plugin_markdown error at {}: {}",
            self.path, self.message
        )
    }
}

impl std::error::Error for WalkPluginMarkdownError {}

/// Recursively walk a plugin directory, invoking on_file for each .md file.
/// Breadcrumbs track directory path segments for context.
pub async fn walk_plugin_markdown<F, Fut>(
    root_dir: &Path,
    on_file: F,
    opts: WalkPluginMarkdownOpts,
) -> std::io::Result<()>
where
    F: Fn(String, Vec<String>) -> Fut + Send + Sync + Clone,
    Fut: std::future::Future<Output = ()> + Send,
{
    let stop_at_skill_dir = opts.stop_at_skill_dir.unwrap_or(false);

    // Collect md files synchronously to avoid recursive async
    let mut md_files: Vec<(String, Vec<String>)> = Vec::new();
    collect_md_files(root_dir, &mut Vec::new(), &mut md_files, &stop_at_skill_dir)
        .map_err(|e| std::io::Error::new(e.kind(), e.to_string()))?;

    for (path_str, crumbs) in md_files {
        on_file(path_str, crumbs).await;
    }

    Ok(())
}

fn collect_md_files(
    dir: &Path,
    breadcrumbs: &mut Vec<String>,
    md_files: &mut Vec<(String, Vec<String>)>,
    stop_at_skill_dir: &bool,
) -> std::io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) => {
            log::debug!("Failed to read directory {:?}: {}", dir, e);
            return Ok(());
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                log::debug!("Failed to read entry in {:?}: {}", dir, e);
                continue;
            }
        };

        let path = entry.path();
        let name_str = entry.file_name().to_string_lossy().to_string();

        // Skip .git and hidden directories
        if name_str.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            // Check if this is a skills directory and should stop
            if *stop_at_skill_dir && name_str == "skills" {
                continue;
            }

            // Add directory name to breadcrumbs and recurse
            breadcrumbs.push(name_str);
            collect_md_files(&path, breadcrumbs, md_files, stop_at_skill_dir)?;
            breadcrumbs.pop();
        } else if let Some(ext) = path.extension() {
            if ext == "md" {
                let path_str = path.to_string_lossy().to_string();
                let crumbs = breadcrumbs.clone();
                md_files.push((path_str, crumbs));
            }
        }
    }

    Ok(())
}
