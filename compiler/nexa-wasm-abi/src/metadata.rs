use crate::target::{ABI_VERSION, LANGUAGE_PROFILE_VERSION, MAGIC, TARGET_IDENTITY};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbiMetadata {
    pub magic: [u8; 4],
    pub abi_version: u32,
    pub target: String,
    pub language_profile: String,
    pub runtime_requirements_fingerprint: String,
}

impl AbiMetadata {
    pub fn new(runtime_requirements_fingerprint: &str) -> Self {
        Self {
            magic: *MAGIC,
            abi_version: ABI_VERSION,
            target: TARGET_IDENTITY.to_string(),
            language_profile: LANGUAGE_PROFILE_VERSION.to_string(),
            runtime_requirements_fingerprint: runtime_requirements_fingerprint.to_string(),
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.magic);
        bytes.extend_from_slice(&self.abi_version.to_le_bytes());
        let target_bytes = self.target.as_bytes();
        bytes.extend_from_slice(&(target_bytes.len() as u32).to_le_bytes());
        bytes.extend_from_slice(target_bytes);
        let profile_bytes = self.language_profile.as_bytes();
        bytes.extend_from_slice(&(profile_bytes.len() as u32).to_le_bytes());
        bytes.extend_from_slice(profile_bytes);
        let fp_bytes = self.runtime_requirements_fingerprint.as_bytes();
        bytes.extend_from_slice(&(fp_bytes.len() as u32).to_le_bytes());
        bytes.extend_from_slice(fp_bytes);
        bytes
    }

    pub fn deserialize(data: &[u8]) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }
        let mut magic = [0u8; 4];
        magic.copy_from_slice(&data[0..4]);
        if magic != *MAGIC {
            return None;
        }
        let mut pos = 4;
        if pos + 4 > data.len() {
            return None;
        }
        let abi_version =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        pos += 4;
        if pos + 4 > data.len() {
            return None;
        }
        let target_len =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;
        if pos + target_len > data.len() {
            return None;
        }
        let target = String::from_utf8(data[pos..pos + target_len].to_vec()).ok()?;
        pos += target_len;
        if pos + 4 > data.len() {
            return None;
        }
        let profile_len =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;
        if pos + profile_len > data.len() {
            return None;
        }
        let language_profile = String::from_utf8(data[pos..pos + profile_len].to_vec()).ok()?;
        pos += profile_len;
        if pos + 4 > data.len() {
            return None;
        }
        let fp_len =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;
        if pos + fp_len > data.len() {
            return None;
        }
        let fingerprint = String::from_utf8(data[pos..pos + fp_len].to_vec()).ok()?;
        Some(Self {
            magic,
            abi_version,
            target,
            language_profile,
            runtime_requirements_fingerprint: fingerprint,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmArtifactMetadata {
    pub abi_version: u32,
    pub target: String,
    pub language_profile: String,
    pub runtime_requirements_fingerprint: String,
    pub compiler_version: String,
}

impl WasmArtifactMetadata {
    pub fn new() -> Self {
        Self {
            abi_version: ABI_VERSION,
            target: TARGET_IDENTITY.to_string(),
            language_profile: LANGUAGE_PROFILE_VERSION.to_string(),
            runtime_requirements_fingerprint: String::new(),
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    pub fn with_fingerprint(mut self, fp: &str) -> Self {
        self.runtime_requirements_fingerprint = fp.to_string();
        self
    }
}

impl Default for WasmArtifactMetadata {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilitySet {
    pub console_write: bool,
    pub runtime_tasks: bool,
}

impl CapabilitySet {
    pub fn baseline() -> Self {
        Self {
            console_write: true,
            runtime_tasks: false,
        }
    }

    pub fn full() -> Self {
        Self {
            console_write: true,
            runtime_tasks: true,
        }
    }
}

impl Default for CapabilitySet {
    fn default() -> Self {
        Self::baseline()
    }
}
