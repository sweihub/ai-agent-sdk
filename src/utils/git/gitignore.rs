// Source: /data/home/swei/claudecode/openclaudecode/src/utils/git/gitignore.ts
use std::io::Write;
use std::path::Path;
use std::process::Command;

/// Checks if a path is ignored by git (via `git check-ignore`).
pub fn is_path_gitignored<P: AsRef<std::path::Path>, C: AsRef<std::path::Path>>(file_path: P, cwd: C) -> bool {
    let output = Command::new("git")
        .args(["check-ignore", file_path.as_ref().to_str().unwrap_or("")])
        .current_dir(cwd.as_ref())
        .output();

    match output {
        Ok(o) => o.status.code() == Some(0),
        Err(_) => false,
    }
}

/// Gets the path to the global gitignore file (.config/git/ignore)
pub fn get_global_gitignore_path() -> String {
    if let Some(home) = std::env::var("HOME").ok() {
        format!("{}/.config/git/ignore", home)
    } else {
        ".config/git/ignore".to_string()
    }
}

/// Checks if a directory is inside a git repo.
pub fn dir_is_in_git_repo(dir: &str) -> bool {
    Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(dir)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Adds a file pattern to the global gitignore file.
pub async fn add_file_glob_rule_to_gitignore(file_name: &str, cwd: &str) {
    if let Err(e) = add_file_glob_rule_inner(file_name, cwd).await {
        eprintln!("Failed to add gitignore rule: {}", e);
    }
}

async fn add_file_glob_rule_inner(file_name: &str, cwd: &str) -> std::io::Result<()> {
    if !dir_is_in_git_repo(cwd) {
        return Ok(());
    }

    let gitignore_entry = format!("**/{}", file_name);
    let test_path = if file_name.ends_with('/') {
        format!("{}sample-file.txt", file_name)
    } else {
        file_name.to_string()
    };

    if is_path_gitignored(&test_path, cwd) {
        return Ok(());
    }

    let global_gitignore_path = get_global_gitignore_path();

    if let Some(parent) = Path::new(&global_gitignore_path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    if let Ok(content) = std::fs::read_to_string(&global_gitignore_path) {
        if content.contains(&gitignore_entry) {
            return Ok(());
        }
        std::fs::OpenOptions::new()
            .append(true)
            .open(&global_gitignore_path)?
            .write_all(format!("\n{}\n", gitignore_entry).as_bytes())?;
    } else {
        std::fs::write(&global_gitignore_path, format!("{}\n", gitignore_entry))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_path_gitignored_not_in_repo() {
        assert!(!is_path_gitignored("some_file.txt", "/tmp"));
    }

    #[test]
    fn test_get_global_gitignore_path() {
        let path = get_global_gitignore_path();
        assert!(path.contains(".config/git/ignore"));
    }
}
