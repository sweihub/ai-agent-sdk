// Source: ~/claudecode/openclaudecode/src/services/autoDream/consolidationLock.ts
//! Lock file whose mtime IS lastConsolidatedAt. Body is the holder's PID.
//!
//! Lives inside the memory dir (get_auto_mem_path) so it keys on git-root
//! like memory does, and so it's writable even when the memory path comes
//! from an env/settings override whose parent may not be.
//!
//! Lock semantics:
//! - mtime = lastConsolidatedAt (0 if no lock file)
//! - body = PID of the holder (for stale detection)
//! - HOLDER_STALE_MS = 1 hour (PID reuse guard)

use crate::memdir::paths::get_auto_mem_path;
use crate::session::{list_sessions, SessionMetadata};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const LOCK_FILE: &str = ".consolidate-lock";
const HOLDER_STALE_MS: u64 = 60 * 60 * 1000; // 1 hour stale past even if PID is live

pub(crate) fn lock_path() -> PathBuf {
    get_auto_mem_path().join(LOCK_FILE)
}

/// mtime of the lock file = lastConsolidatedAt. 0 if absent.
/// Per-turn cost: one stat.
pub async fn read_last_consolidated_at() -> u64 {
    let path = lock_path();
    match fs::metadata(&path) {
        Ok(metadata) => {
            metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0)
        }
        Err(_) => 0,
    }
}

/// Acquire: write PID -> mtime = now. Returns the pre-acquire mtime
/// (for rollback), or None if blocked / lost a race.
///
///   Success -> mtime stays at now.
///   Failure -> rollback_consolidation_lock(prior_mtime) rewinds mtime.
///   Crash   -> mtime stuck, dead PID -> next process reclaims.
pub async fn try_acquire_consolidation_lock() -> Option<u64> {
    let path = lock_path();

    let (mtime_ms, holder_pid) = match fs::metadata(&path) {
        Ok(metadata) => {
            let mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64);
            let pid = fs::read_to_string(&path)
                .ok()
                .and_then(|raw| raw.trim().parse::<i32>().ok());
            (mtime, pid)
        }
        Err(_) => {
            // ENOENT - no prior lock.
            (None, None)
        }
    };

    // Check if existing lock is stale and the holder PID is dead
    if let (Some(mtime), Some(pid)) = (mtime_ms, holder_pid) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        if now - mtime < HOLDER_STALE_MS {
            log::debug!(
                "[autoDream] lock held by PID {} (mtime {}s ago), skipping",
                pid,
                ((now - mtime) / 1000)
            );
            return None;
        }
        // Dead PID or unparseable body - reclaim.
    }

    // Memory dir may not exist yet.
    if let Err(e) = fs::create_dir_all(get_auto_mem_path()) {
        log::debug!("[autoDream] create memory dir for lock failed: {}", e);
        return None;
    }

    // Write PID to lock file
    let current_pid = std::process::id();
    let pid_str = current_pid.to_string();
    let mut file = match fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
    {
        Ok(f) => f,
        Err(e) => {
            log::debug!("[autoDream] write lock file failed: {}", e);
            return None;
        }
    };
    if let Err(e) = file.write_all(pid_str.as_bytes()) {
        log::debug!("[autoDream] write lock body failed: {}", e);
        return None;
    }

    // Two reclaimers both write -> last wins the PID. Loser bails on re-read.
    let verify = match fs::read_to_string(&path) {
        Ok(v) => v,
        Err(_) => return None,
    };
    if verify.trim().parse::<u32>().ok() != Some(current_pid) {
        return None;
    }

    Some(mtime_ms.unwrap_or(0))
}

/// Rewind mtime to pre-acquire after a failed fork. Clears the PID body.
/// prior_mtime 0 -> unlink (restore no-file).
pub async fn rollback_consolidation_lock(prior_mtime: u64) -> Result<(), String> {
    let path = lock_path();
    match prior_mtime {
        0 => {
            if let Err(e) = fs::remove_file(&path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    log::debug!(
                        "[autoDream] rollback unlink failed: {} — next trigger delayed to minHours",
                        e
                    );
                }
            }
        }
        _ => {
            if let Err(e) = fs::write(&path, "") {
                log::debug!(
                    "[autoDream] rollback clear body failed: {} — next trigger delayed to minHours",
                    e
                );
                return Err(format!("rollback clear body: {e}"));
            }

            // Restore the prior mtime using utimes (C library function).
            // utimes expects timeval (seconds + microseconds).
            let secs = (prior_mtime / 1000) as libc::time_t;
            let usecs = ((prior_mtime % 1000) * 1_000) as libc::suseconds_t;

            let times = [
                libc::timeval { tv_sec: secs, tv_usec: usecs },
                libc::timeval { tv_sec: secs, tv_usec: usecs },
            ];

            let c_path = std::ffi::CString::new(path.to_string_lossy().as_bytes())
                .map_err(|e| format!("rollback path conversion: {e}"))?;
            let ret = unsafe { libc::utimes(c_path.as_ptr(), times.as_ptr() as *const libc::timeval) };
            if ret != 0 {
                let err = std::io::Error::last_os_error();
                log::debug!(
                    "[autoDream] rollback utimes failed: {} — next trigger delayed to minHours",
                    err
                );
                return Err(format!("rollback utimes: {err}"));
            }
        }
    }
    Ok(())
}

/// Session IDs with mtime after since_ms. Caller excludes the current session.
pub async fn list_sessions_touched_since(since_ms: u64) -> Vec<String> {
    match list_sessions().await {
        Ok(sessions) => sessions
            .into_iter()
            .filter_map(|s| {
                let updated = chrono::DateTime::parse_from_rfc3339(&s.updated_at).ok()?;
                let updated_ms = updated.timestamp_millis() as u64;
                if updated_ms > since_ms {
                    Some(s.id)
                } else {
                    None
                }
            })
            .collect(),
        Err(e) => {
            log::debug!("[autoDream] listSessionsTouchedSince failed: {}", e);
            Vec::new()
        }
    }
}

/// Stamp from manual /dream. Optimistic — fires at prompt-build time.
pub async fn record_consolidation() -> Result<(), String> {
    if let Err(e) = fs::create_dir_all(get_auto_mem_path()) {
        return Err(format!("create memory dir: {e}"));
    }
    let path = lock_path();
    let current_pid = std::process::id().to_string();
    if let Err(e) = fs::write(&path, current_pid) {
        log::debug!("[autoDream] recordConsolidation write failed: {}", e);
        return Err(format!("write lock: {e}"));
    }
    Ok(())
}
