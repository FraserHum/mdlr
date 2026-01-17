use crate::graph::Graph;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Target {
    Directory(PathBuf),
    File(PathBuf),
    Object { file: PathBuf, name: String },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub targets: Vec<Target>,
    pub graph: Graph,
}

impl Session {
    pub fn new(id: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            created_at: now,
            updated_at: now,
            targets: Vec::new(),
            graph: Graph::new(),
        }
    }

    pub fn add_target(&mut self, target: Target) {
        self.targets.push(target);
        self.updated_at = Utc::now();
    }

    pub fn clear_targets(&mut self) {
        self.targets.clear();
        self.updated_at = Utc::now();
    }

    pub fn update_graph(&mut self, graph: Graph) {
        self.graph = graph;
        self.updated_at = Utc::now();
    }
}

pub struct SessionStore {
    base_dir: PathBuf,
}

impl SessionStore {
    pub fn new() -> Result<Self> {
        let base_dir = dirs::cache_dir()
            .context("Could not determine cache directory")?
            .join("mdlr")
            .join("sessions");
        fs::create_dir_all(&base_dir)
            .with_context(|| format!("Failed to create sessions directory: {:?}", base_dir))?;
        Ok(Self { base_dir })
    }

    fn session_path(&self, id: &str) -> PathBuf {
        self.base_dir.join(format!("{}.json", id))
    }

    pub fn create(&self, id: &str) -> Result<Session> {
        let path = self.session_path(id);
        if path.exists() {
            anyhow::bail!("Session '{}' already exists", id);
        }
        let session = Session::new(id.to_string());
        self.save(&session)?;
        Ok(session)
    }

    pub fn load(&self, id: &str) -> Result<Session> {
        let path = self.session_path(id);
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Session '{}' not found", id))?;
        let session: Session = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse session '{}'", id))?;
        Ok(session)
    }

    pub fn save(&self, session: &Session) -> Result<()> {
        let path = self.session_path(&session.id);
        let content = serde_json::to_string_pretty(session)?;
        fs::write(&path, content)
            .with_context(|| format!("Failed to save session '{}'", session.id))?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let path = self.session_path(id);
        if !path.exists() {
            anyhow::bail!("Session '{}' not found", id);
        }
        fs::remove_file(&path)
            .with_context(|| format!("Failed to delete session '{}'", id))?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<String>> {
        let mut sessions = Vec::new();
        for entry in fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                if let Some(stem) = path.file_stem() {
                    sessions.push(stem.to_string_lossy().to_string());
                }
            }
        }
        sessions.sort();
        Ok(sessions)
    }

    pub fn exists(&self, id: &str) -> bool {
        self.session_path(id).exists()
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new().expect("Failed to create session store")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn temp_store() -> SessionStore {
        let temp_dir = env::temp_dir().join(format!("mdlr-test-{}", uuid()));
        fs::create_dir_all(&temp_dir).unwrap();
        SessionStore { base_dir: temp_dir }
    }

    fn uuid() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        format!("{}{}", duration.as_secs(), duration.subsec_nanos())
    }

    #[test]
    fn test_create_and_load() {
        let store = temp_store();
        let session = store.create("test-session").unwrap();
        assert_eq!(session.id, "test-session");

        let loaded = store.load("test-session").unwrap();
        assert_eq!(loaded.id, "test-session");
    }

    #[test]
    fn test_delete() {
        let store = temp_store();
        store.create("to-delete").unwrap();
        assert!(store.exists("to-delete"));

        store.delete("to-delete").unwrap();
        assert!(!store.exists("to-delete"));
    }

    #[test]
    fn test_list() {
        let store = temp_store();
        store.create("session-a").unwrap();
        store.create("session-b").unwrap();

        let list = store.list().unwrap();
        assert!(list.contains(&"session-a".to_string()));
        assert!(list.contains(&"session-b".to_string()));
    }
}
