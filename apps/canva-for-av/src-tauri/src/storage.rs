use crate::models::RecentEntry;
use std::path::PathBuf;

fn recent_file() -> PathBuf {
    let dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("canva-for-av");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("recent.json")
}

pub fn load_recent() -> Vec<RecentEntry> {
    let path = recent_file();
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub fn save_recent(entries: &[RecentEntry]) -> Result<(), String> {
    let path = recent_file();
    let content =
        serde_json::to_string_pretty(entries).map_err(|e| format!("Failed to serialize: {}", e))?;
    std::fs::write(&path, content).map_err(|e| format!("Failed to write recent.json: {}", e))
}

pub fn add_recent(entry: RecentEntry) -> Result<(), String> {
    let mut entries = load_recent();
    // Remove existing entry with same ID
    entries.retain(|e| e.id != entry.id);
    // Add to front
    entries.insert(0, entry);
    // Keep at most 20
    entries.truncate(20);
    save_recent(&entries)
}

pub fn remove_recent(id: &str) -> Result<(), String> {
    let mut entries = load_recent();
    entries.retain(|e| e.id != id);
    save_recent(&entries)
}
