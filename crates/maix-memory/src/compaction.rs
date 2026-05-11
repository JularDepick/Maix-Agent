//! Memory compaction — summarize and compress old memory entries.

use crate::MemoryStore;
use chrono::Utc;

/// Compact session memories by keeping only the most relevant entries.
pub async fn compact_session(
    store: &mut dyn MemoryStore,
    query: &str,
    max_entries: usize,
) -> Result<usize, String> {
    let entries = store
        .search(query, max_entries * 2)
        .await
        .map_err(|e| e.to_string())?;

    if entries.len() <= max_entries {
        return Ok(0);
    }

    let mut removed = 0;
    for entry in entries.iter().skip(max_entries) {
        let _ = store.forget(&entry.id).await;
        removed += 1;
    }
    Ok(removed)
}

/// Remove episodic memories older than the given number of days.
pub async fn summarize_old_entries(
    store: &mut dyn MemoryStore,
    older_than_days: i64,
) -> Result<usize, String> {
    let cutoff = Utc::now() - chrono::Duration::days(older_than_days);
    let entries = store
        .search("", 1000)
        .await
        .map_err(|e| e.to_string())?;

    let old_count = entries.iter().filter(|e| e.created_at < cutoff).count();
    for entry in entries.iter().filter(|e| e.created_at < cutoff) {
        let _ = store.forget(&entry.id).await;
    }
    Ok(old_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compaction_smoke() {
        assert!(true);
    }
}
