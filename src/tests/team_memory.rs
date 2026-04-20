use crate::team_memory::*;
use std::collections::HashMap;

#[test]
fn test_create_sync_state() {
    let state = create_sync_state();
    assert!(state.last_known_checksum.is_none());
    assert!(state.server_checksums.is_empty());
    assert!(state.server_max_entries.is_none());
}

#[test]
fn test_hash_content() {
    let hash1 = hash_content("hello");
    let hash2 = hash_content("hello");
    let hash3 = hash_content("world");

    assert!(hash1.starts_with("sha256:"));
    assert_eq!(hash1, hash2);
    assert_ne!(hash1, hash3);

    // Verify it's actual SHA-256 (64 hex chars after "sha256:")
    let hex_part = hash1.strip_prefix("sha256:").unwrap();
    assert_eq!(hex_part.len(), 64); // SHA-256 produces 256 bits = 64 hex chars
}

#[test]
fn test_hash_content_known_value() {
    // SHA-256 of "hello" is a known value
    let hash = hash_content("hello");
    let hex_part = hash.strip_prefix("sha256:").unwrap();
    // SHA-256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
    assert_eq!(
        hex_part,
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
}

#[test]
fn test_validate_team_memory_key() {
    assert!(validate_team_memory_key("MEMORY.md").is_ok());
    assert!(validate_team_memory_key("subdir/notes.md").is_ok());
    assert!(validate_team_memory_key("").is_err());
    assert!(validate_team_memory_key("../etc/passwd").is_err());
    assert!(validate_team_memory_key("/absolute/path").is_err());
}

#[test]
fn test_compute_delta() {
    let local = HashMap::from([
        ("a.txt".to_string(), "content1".to_string()),
        ("b.txt".to_string(), "content2".to_string()),
        ("c.txt".to_string(), "content3".to_string()),
    ]);

    let server = HashMap::from([
        ("a.txt".to_string(), hash_content("content1")), // Same
        ("b.txt".to_string(), hash_content("different")), // Different
    ]);

    let delta = compute_delta(&local, &server);

    assert!(delta.contains_key("b.txt")); // Changed
    assert!(delta.contains_key("c.txt")); // New
    assert!(!delta.contains_key("a.txt")); // Unchanged
}

#[test]
fn test_batch_delta_by_bytes() {
    let delta = HashMap::from([
        ("a.txt".to_string(), "x".repeat(100)),
        ("b.txt".to_string(), "y".repeat(100)),
        ("c.txt".to_string(), "z".repeat(250)), // Oversized
    ]);

    let batches = batch_delta_by_bytes(&delta, 150);

    // c.txt should be in its own batch (oversized)
    // a.txt and b.txt should be together
    assert!(batches.len() >= 2);
}

#[test]
fn test_team_memory_enabled() {
    disable_team_memory();
    assert!(!is_team_memory_enabled());

    enable_team_memory();
    assert!(is_team_memory_enabled());

    disable_team_memory();
    assert!(!is_team_memory_enabled());
}

#[test]
fn test_last_sync_error() {
    set_last_sync_error(None);
    assert!(get_last_sync_error().is_none());

    set_last_sync_error(Some("test error".to_string()));
    assert_eq!(get_last_sync_error(), Some("test error".to_string()));
}

#[test]
fn test_is_team_memory_sync_available_no_token() {
    // Without OAuth token env var set, should return false
    assert!(!is_team_memory_sync_available());
}

#[test]
fn test_scan_entries_for_secrets_empty() {
    let entries = HashMap::new();
    let skipped = scan_entries_for_secrets(&entries);
    assert!(skipped.is_empty());
}

#[test]
fn test_scan_for_secrets_returns_none() {
    // Placeholder implementation returns None
    assert!(scan_for_secrets("safe content", "safe.txt").is_none());
}
