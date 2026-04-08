use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

#[derive(Deserialize, Debug)]
struct UserCacheEntry {
    name: String,
    uuid: String,
}

#[derive(Debug, Clone)]
pub struct UsernameCache {
    uuid_to_name: HashMap<Uuid, String>,
    name_to_uuid: HashMap<String, Uuid>,
}

impl UsernameCache {
    pub fn from_usercache(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let entries: Vec<UserCacheEntry> = serde_json::from_str(&content)?;

        let mut uuid_to_name = HashMap::new();
        let mut name_to_uuid = HashMap::new();

        for entry in entries {
            if let Ok(uuid) = Uuid::parse_str(&entry.uuid) {
                uuid_to_name.insert(uuid, entry.name.clone());
                name_to_uuid.insert(entry.name, uuid);
            }
        }

        Ok(Self {
            uuid_to_name,
            name_to_uuid,
        })
    }

    pub fn username_to_uuid(&self, name: &String) -> Option<Uuid> {
        self.name_to_uuid.get(name).cloned()
    }

    pub fn uuid_to_username(&self, uuid: &Uuid) -> Option<String> {
        self.uuid_to_name.get(uuid).cloned()
    }

    pub fn len(&self) -> usize {
        self.uuid_to_name.len()
    }
}
