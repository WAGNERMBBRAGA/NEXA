use crate::effect_id::EffectId;
use crate::effect_set::EffectSet;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct EffectEntry {
    pub id: EffectId,
    pub path: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct EffectRegistry {
    effects: BTreeMap<String, EffectEntry>,
    next_id: u32,
}

impl EffectRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            effects: BTreeMap::new(),
            next_id: 0,
        };
        registry.register_standard_effects();
        registry
    }

    fn register_standard_effects(&mut self) {
        let standard_effects = [
            ("console::write", "Console write output"),
            ("filesystem::read", "Read from filesystem"),
            ("filesystem::write", "Write to filesystem"),
            ("network::request", "Network request"),
            ("clock::wall", "Wall clock access"),
            ("clock::monotonic", "Monotonic clock access"),
            ("random::secure", "Cryptographic random generation"),
            ("process::spawn", "Process spawning"),
            ("process::shell", "Shell execution"),
            ("environment::read", "Environment variable reading"),
            ("observe::emit", "Observability/telemetry emission"),
            ("database::read", "Database read operation"),
            ("database::write", "Database write operation"),
            ("crypto::operation", "Cryptographic operation"),
            ("secret::read", "Secret/credential reading"),
            ("secret::write", "Secret/credential writing"),
            ("ai::inference", "AI inference operation"),
            ("device::read", "Device reading"),
            ("device::control", "Device control"),
            ("notification::send", "Notification sending"),
            ("physical::actuate", "Physical actuation"),
            ("financial::transact", "Financial transaction"),
        ];

        for (path, description) in standard_effects {
            let id = EffectId(self.next_id);
            self.next_id += 1;
            self.effects.insert(
                path.to_string(),
                EffectEntry {
                    id,
                    path: path.to_string(),
                    description: description.to_string(),
                },
            );
        }
    }

    pub fn register(&mut self, path: String, description: String) -> EffectId {
        if let Some(entry) = self.effects.get(&path) {
            return entry.id;
        }
        let id = EffectId(self.next_id);
        self.next_id += 1;
        self.effects.insert(
            path.clone(),
            EffectEntry {
                id,
                path,
                description,
            },
        );
        id
    }

    pub fn resolve(&self, path: &str) -> Option<EffectId> {
        self.effects.get(path).map(|e| e.id)
    }

    pub fn resolve_set(&self, paths: &[String]) -> Result<EffectSet, Vec<String>> {
        let mut set = EffectSet::new();
        let mut unknown = Vec::new();
        for path in paths {
            match self.resolve(path) {
                Some(id) => {
                    set.insert(id);
                }
                None => {
                    unknown.push(path.clone());
                }
            }
        }
        if unknown.is_empty() {
            Ok(set)
        } else {
            Err(unknown)
        }
    }

    pub fn entry(&self, id: EffectId) -> Option<&EffectEntry> {
        self.effects.values().find(|e| e.id == id)
    }

    pub fn entry_by_path(&self, path: &str) -> Option<&EffectEntry> {
        self.effects.get(path)
    }

    pub fn all_entries(&self) -> impl Iterator<Item = &EffectEntry> {
        self.effects.values()
    }

    pub fn count(&self) -> usize {
        self.effects.len()
    }
}

impl Default for EffectRegistry {
    fn default() -> Self {
        Self::new()
    }
}
