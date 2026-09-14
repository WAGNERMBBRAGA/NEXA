use std::fmt;

// ---------------------------------------------------------------------------
// ResourceTypeId
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy, PartialOrd, Ord)]
pub struct ResourceTypeId(pub u32);

impl ResourceTypeId {
    pub const FILE: Self = Self(1);
    pub const DIRECTORY: Self = Self(2);
    pub const SOCKET: Self = Self(3);
    pub const PROCESS: Self = Self(4);
    pub const SECRET: Self = Self(5);
    pub const TASK: Self = Self(6);
}

// ---------------------------------------------------------------------------
// ResourceSlotIndex
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceSlotIndex(pub usize);

// ---------------------------------------------------------------------------
// ResourceHandle
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceHandle {
    pub(crate) slot_index: ResourceSlotIndex,
    pub(crate) generation: u32,
    pub(crate) resource_type: ResourceTypeId,
}

impl ResourceHandle {
    pub fn slot_index(&self) -> ResourceSlotIndex {
        self.slot_index
    }

    pub fn generation(&self) -> u32 {
        self.generation
    }

    pub fn resource_type(&self) -> ResourceTypeId {
        self.resource_type
    }

    /// Pack into a single i64: high 32 bits = slot index, low 32 bits = generation.
    pub fn to_i64(&self) -> i64 {
        ((self.slot_index.0 as u64) << 32 | self.generation as u64) as i64
    }

    /// Unpack from i64. Returns `None` if negative or generation overflow.
    pub fn from_i64(v: i64) -> Option<Self> {
        if v < 0 {
            return None;
        }
        let bits = v as u64;
        let slot = (bits >> 32) as u32;
        let generation = bits as u32;
        Some(Self {
            slot_index: ResourceSlotIndex(slot as usize),
            generation,
            resource_type: ResourceTypeId(0),
        })
    }
}

// ---------------------------------------------------------------------------
// ResourceState
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceState {
    Open,
    Closing,
    Closed,
    Revoked,
    Expired,
    Failed,
}

// ---------------------------------------------------------------------------
// HandleMode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleMode {
    Revocable,
    LeaseBound,
    LifetimeBound,
}

// ---------------------------------------------------------------------------
// ObjectAuthority
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectAuthority {
    pub context_id: u64,
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl ObjectAuthority {
    pub fn read_only(context_id: u64) -> Self {
        Self {
            context_id,
            read: true,
            write: false,
            execute: false,
        }
    }

    pub fn read_write(context_id: u64) -> Self {
        Self {
            context_id,
            read: true,
            write: true,
            execute: false,
        }
    }

    pub fn full(context_id: u64) -> Self {
        Self {
            context_id,
            read: true,
            write: true,
            execute: true,
        }
    }
}

// ---------------------------------------------------------------------------
// ResourceSlot
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ResourceSlot {
    generation: u32,
    resource_type: ResourceTypeId,
    owner_context_id: u64,
    authority: ObjectAuthority,
    state: ResourceState,
    mode: HandleMode,
    lease_expiry: Option<u64>,
}

// ---------------------------------------------------------------------------
// ResourceError
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum ResourceError {
    LimitExceeded {
        resource: String,
        limit: u32,
    },
    InvalidHandle {
        handle: i64,
    },
    StaleGeneration {
        expected: u32,
        actual: u32,
    },
    WrongType {
        expected: ResourceTypeId,
        actual: ResourceTypeId,
    },
    NotOpen {
        handle: i64,
    },
    AlreadyClosed {
        handle: i64,
    },
    LeaseExpired {
        handle: i64,
    },
    Unauthorized {
        handle: i64,
        detail: String,
    },
}

impl fmt::Display for ResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LimitExceeded { resource, limit } => {
                write!(
                    f,
                    "NEXA-NXR: limit exceeded for '{}' (limit={})",
                    resource, limit
                )
            }
            Self::InvalidHandle { handle } => {
                write!(f, "NEXA-NXR: invalid handle {}", handle)
            }
            Self::StaleGeneration { expected, actual } => {
                write!(
                    f,
                    "NEXA-NXR: stale generation (expected={}, actual={})",
                    expected, actual
                )
            }
            Self::WrongType { expected, actual } => {
                write!(
                    f,
                    "NEXA-NXR: wrong type (expected={}, actual={})",
                    expected.0, actual.0
                )
            }
            Self::NotOpen { handle } => {
                write!(f, "NEXA-NXR: handle {} is not open", handle)
            }
            Self::AlreadyClosed { handle } => {
                write!(f, "NEXA-NXR: handle {} already closed", handle)
            }
            Self::LeaseExpired { handle } => {
                write!(f, "NEXA-NXR: lease expired for handle {}", handle)
            }
            Self::Unauthorized { handle, detail } => {
                write!(f, "NEXA-NXR: unauthorized handle {} ({})", handle, detail)
            }
        }
    }
}

impl std::error::Error for ResourceError {}

// ---------------------------------------------------------------------------
// ResourceRegistry
// ---------------------------------------------------------------------------

pub struct ResourceRegistry {
    slots: Vec<Option<ResourceSlot>>,
    free: Vec<usize>,
    open_count: u32,
    max_open: u32,
}

impl ResourceRegistry {
    pub fn new(max_open: u32) -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            open_count: 0,
            max_open,
        }
    }

    pub fn allocate(
        &mut self,
        resource_type: ResourceTypeId,
        owner_context_id: u64,
        authority: ObjectAuthority,
        mode: HandleMode,
    ) -> Result<ResourceHandle, ResourceError> {
        if self.open_count >= self.max_open {
            return Err(ResourceError::LimitExceeded {
                resource: format!("type_{}", resource_type.0),
                limit: self.max_open,
            });
        }

        let slot_index = if let Some(idx) = self.free.pop() {
            self.slots[idx] = Some(ResourceSlot {
                generation: 0,
                resource_type,
                owner_context_id,
                authority,
                state: ResourceState::Open,
                mode,
                lease_expiry: None,
            });
            idx
        } else {
            let idx = self.slots.len();
            self.slots.push(Some(ResourceSlot {
                generation: 0,
                resource_type,
                owner_context_id,
                authority,
                state: ResourceState::Open,
                mode,
                lease_expiry: None,
            }));
            idx
        };

        self.open_count += 1;

        Ok(ResourceHandle {
            slot_index: ResourceSlotIndex(slot_index),
            generation: 0,
            resource_type,
        })
    }

    pub fn validate(&self, handle: &ResourceHandle) -> Result<ResourceSlot, ResourceError> {
        let idx = handle.slot_index.0;
        if idx >= self.slots.len() {
            return Err(ResourceError::InvalidHandle {
                handle: handle.to_i64(),
            });
        }
        let slot = self.slots[idx]
            .as_ref()
            .ok_or_else(|| ResourceError::InvalidHandle {
                handle: handle.to_i64(),
            })?;

        if slot.generation != handle.generation {
            return Err(ResourceError::StaleGeneration {
                expected: slot.generation,
                actual: handle.generation,
            });
        }

        if slot.resource_type != handle.resource_type {
            return Err(ResourceError::WrongType {
                expected: slot.resource_type,
                actual: handle.resource_type,
            });
        }

        if slot.state != ResourceState::Open {
            return Err(ResourceError::NotOpen {
                handle: handle.to_i64(),
            });
        }

        Ok(slot.clone())
    }

    pub fn release(&mut self, handle: ResourceHandle) -> Result<(), ResourceError> {
        let idx = handle.slot_index.0;
        if idx >= self.slots.len() {
            return Err(ResourceError::InvalidHandle {
                handle: handle.to_i64(),
            });
        }
        let slot = self.slots[idx]
            .as_mut()
            .ok_or_else(|| ResourceError::InvalidHandle {
                handle: handle.to_i64(),
            })?;

        if slot.state == ResourceState::Closed {
            return Err(ResourceError::AlreadyClosed {
                handle: handle.to_i64(),
            });
        }
        if slot.state != ResourceState::Open && slot.state != ResourceState::Revoked {
            return Err(ResourceError::NotOpen {
                handle: handle.to_i64(),
            });
        }

        if slot.state == ResourceState::Open {
            self.open_count -= 1;
        }
        slot.state = ResourceState::Closed;
        slot.generation += 1;
        self.free.push(idx);
        Ok(())
    }

    pub fn revoke(&mut self, handle: ResourceHandle) -> Result<(), ResourceError> {
        let idx = handle.slot_index.0;
        if idx >= self.slots.len() {
            return Err(ResourceError::InvalidHandle {
                handle: handle.to_i64(),
            });
        }
        let slot = self.slots[idx]
            .as_mut()
            .ok_or_else(|| ResourceError::InvalidHandle {
                handle: handle.to_i64(),
            })?;

        if slot.state != ResourceState::Open {
            return Err(ResourceError::NotOpen {
                handle: handle.to_i64(),
            });
        }

        slot.state = ResourceState::Revoked;
        Ok(())
    }

    pub fn is_open(&self, handle: &ResourceHandle) -> bool {
        let idx = handle.slot_index.0;
        if idx >= self.slots.len() {
            return false;
        }
        match &self.slots[idx] {
            Some(slot) => slot.state == ResourceState::Open && slot.generation == handle.generation,
            None => false,
        }
    }

    pub fn open_count(&self) -> u32 {
        self.open_count
    }

    pub fn max_open(&self) -> u32 {
        self.max_open
    }

    pub fn set_lease_expiry(&mut self, handle: ResourceHandle, expiry: Option<u64>) {
        let idx = handle.slot_index.0;
        if let Some(Some(slot)) = self.slots.get_mut(idx) {
            slot.lease_expiry = expiry;
        }
    }

    pub fn check_lease(
        &self,
        handle: &ResourceHandle,
        current_time: u64,
    ) -> Result<(), ResourceError> {
        let idx = handle.slot_index.0;
        if idx >= self.slots.len() {
            return Err(ResourceError::InvalidHandle {
                handle: handle.to_i64(),
            });
        }
        let slot = self.slots[idx]
            .as_ref()
            .ok_or_else(|| ResourceError::InvalidHandle {
                handle: handle.to_i64(),
            })?;

        if let Some(expiry) = slot.lease_expiry {
            if current_time > expiry {
                return Err(ResourceError::LeaseExpired {
                    handle: handle.to_i64(),
                });
            }
        }
        Ok(())
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_type_id_constants() {
        assert_eq!(ResourceTypeId::FILE.0, 1);
        assert_eq!(ResourceTypeId::DIRECTORY.0, 2);
        assert_eq!(ResourceTypeId::SOCKET.0, 3);
        assert_eq!(ResourceTypeId::PROCESS.0, 4);
        assert_eq!(ResourceTypeId::SECRET.0, 5);
        assert_eq!(ResourceTypeId::TASK.0, 6);
    }

    #[test]
    fn test_resource_handle_packing() {
        let h = ResourceHandle {
            slot_index: ResourceSlotIndex(42),
            generation: 7,
            resource_type: ResourceTypeId::FILE,
        };
        let packed = h.to_i64();
        let unpacked = ResourceHandle::from_i64(packed).unwrap();
        assert_eq!(unpacked.slot_index, ResourceSlotIndex(42));
        assert_eq!(unpacked.generation, 7);
    }

    #[test]
    fn test_resource_handle_packing_negative() {
        assert!(ResourceHandle::from_i64(-1).is_none());
    }

    #[test]
    fn test_registry_allocate() {
        let mut reg = ResourceRegistry::new(10);
        let auth = ObjectAuthority::read_only(1);
        let h = reg
            .allocate(ResourceTypeId::FILE, 1, auth, HandleMode::Revocable)
            .unwrap();
        assert_eq!(h.slot_index(), ResourceSlotIndex(0));
        assert_eq!(h.generation(), 0);
        assert_eq!(h.resource_type(), ResourceTypeId::FILE);
    }

    #[test]
    fn test_registry_validate_open() {
        let mut reg = ResourceRegistry::new(10);
        let auth = ObjectAuthority::full(1);
        let h = reg
            .allocate(ResourceTypeId::FILE, 1, auth, HandleMode::Revocable)
            .unwrap();
        let slot = reg.validate(&h).unwrap();
        assert_eq!(slot.state, ResourceState::Open);
    }

    #[test]
    fn test_registry_release() {
        let mut reg = ResourceRegistry::new(10);
        let auth = ObjectAuthority::read_write(1);
        let h = reg
            .allocate(
                ResourceTypeId::DIRECTORY,
                1,
                auth,
                HandleMode::LifetimeBound,
            )
            .unwrap();
        assert_eq!(reg.open_count(), 1);
        reg.release(h).unwrap();
        assert_eq!(reg.open_count(), 0);
        assert!(!reg.is_open(&h));
    }

    #[test]
    fn test_registry_stale_generation_rejected() {
        let mut reg = ResourceRegistry::new(10);
        let auth = ObjectAuthority::read_only(1);
        let h = reg
            .allocate(ResourceTypeId::FILE, 1, auth, HandleMode::Revocable)
            .unwrap();
        reg.release(h).unwrap();
        let result = reg.validate(&h);
        assert!(result.is_err());
        match result.unwrap_err() {
            ResourceError::StaleGeneration { .. } => {}
            other => panic!("expected StaleGeneration, got {:?}", other),
        }
    }

    #[test]
    fn test_registry_revoke() {
        let mut reg = ResourceRegistry::new(10);
        let auth = ObjectAuthority::full(1);
        let h = reg
            .allocate(ResourceTypeId::SOCKET, 1, auth, HandleMode::Revocable)
            .unwrap();
        reg.revoke(h).unwrap();
        let slot = reg.validate(&h).unwrap_err();
        assert!(matches!(slot, ResourceError::NotOpen { .. }));
    }

    #[test]
    fn test_registry_limit_exceeded() {
        let mut reg = ResourceRegistry::new(1);
        let auth = ObjectAuthority::read_only(1);
        let _h1 = reg
            .allocate(ResourceTypeId::FILE, 1, auth, HandleMode::Revocable)
            .unwrap();
        let auth2 = ObjectAuthority::read_only(2);
        let result = reg.allocate(ResourceTypeId::FILE, 2, auth2, HandleMode::Revocable);
        assert!(result.is_err());
        match result.unwrap_err() {
            ResourceError::LimitExceeded { .. } => {}
            other => panic!("expected LimitExceeded, got {:?}", other),
        }
    }

    #[test]
    fn test_registry_double_release_rejected() {
        let mut reg = ResourceRegistry::new(10);
        let auth = ObjectAuthority::read_only(1);
        let h = reg
            .allocate(ResourceTypeId::FILE, 1, auth, HandleMode::Revocable)
            .unwrap();
        reg.release(h).unwrap();
        let result = reg.release(h);
        assert!(result.is_err());
    }

    #[test]
    fn test_registry_wrong_type_rejected() {
        let mut reg = ResourceRegistry::new(10);
        let auth = ObjectAuthority::read_only(1);
        let h = reg
            .allocate(ResourceTypeId::FILE, 1, auth, HandleMode::Revocable)
            .unwrap();
        let bad_handle = ResourceHandle {
            slot_index: h.slot_index,
            generation: h.generation,
            resource_type: ResourceTypeId::DIRECTORY,
        };
        let result = reg.validate(&bad_handle);
        assert!(result.is_err());
        match result.unwrap_err() {
            ResourceError::WrongType { .. } => {}
            other => panic!("expected WrongType, got {:?}", other),
        }
    }

    #[test]
    fn test_registry_open_count() {
        let mut reg = ResourceRegistry::new(10);
        let auth = ObjectAuthority::read_only(1);
        assert_eq!(reg.open_count(), 0);
        let h1 = reg
            .allocate(ResourceTypeId::FILE, 1, auth, HandleMode::Revocable)
            .unwrap();
        assert_eq!(reg.open_count(), 1);
        let h2 = reg
            .allocate(ResourceTypeId::FILE, 1, auth, HandleMode::Revocable)
            .unwrap();
        assert_eq!(reg.open_count(), 2);
        reg.release(h1).unwrap();
        assert_eq!(reg.open_count(), 1);
        reg.release(h2).unwrap();
        assert_eq!(reg.open_count(), 0);
    }

    #[test]
    fn test_registry_lease_expiry() {
        let mut reg = ResourceRegistry::new(10);
        let auth = ObjectAuthority::read_only(1);
        let h = reg
            .allocate(ResourceTypeId::FILE, 1, auth, HandleMode::LeaseBound)
            .unwrap();
        reg.set_lease_expiry(h, Some(100));
        assert!(reg.check_lease(&h, 50).is_ok());
        assert!(reg.check_lease(&h, 100).is_ok());
        let result = reg.check_lease(&h, 101);
        assert!(result.is_err());
        match result.unwrap_err() {
            ResourceError::LeaseExpired { .. } => {}
            other => panic!("expected LeaseExpired, got {:?}", other),
        }
    }

    #[test]
    fn test_registry_handle_mode_variants() {
        let mut reg = ResourceRegistry::new(10);
        let auth = ObjectAuthority::read_only(1);
        let h1 = reg
            .allocate(ResourceTypeId::FILE, 1, auth, HandleMode::Revocable)
            .unwrap();
        assert!(reg.is_open(&h1));
        let h2 = reg
            .allocate(ResourceTypeId::FILE, 1, auth, HandleMode::LeaseBound)
            .unwrap();
        assert!(reg.is_open(&h2));
        let h3 = reg
            .allocate(ResourceTypeId::FILE, 1, auth, HandleMode::LifetimeBound)
            .unwrap();
        assert!(reg.is_open(&h3));
    }

    #[test]
    fn test_object_authority_read_only() {
        let a = ObjectAuthority::read_only(42);
        assert_eq!(a.context_id, 42);
        assert!(a.read);
        assert!(!a.write);
        assert!(!a.execute);
    }

    #[test]
    fn test_object_authority_read_write() {
        let a = ObjectAuthority::read_write(7);
        assert_eq!(a.context_id, 7);
        assert!(a.read);
        assert!(a.write);
        assert!(!a.execute);
    }

    #[test]
    fn test_object_authority_full() {
        let a = ObjectAuthority::full(99);
        assert_eq!(a.context_id, 99);
        assert!(a.read);
        assert!(a.write);
        assert!(a.execute);
    }

    #[test]
    fn test_resource_error_display() {
        let e1 = ResourceError::LimitExceeded {
            resource: "file".into(),
            limit: 10,
        };
        assert!(format!("{}", e1).contains("NEXA-NXR"));

        let e2 = ResourceError::InvalidHandle { handle: -1 };
        assert!(format!("{}", e2).contains("NEXA-NXR"));

        let e3 = ResourceError::StaleGeneration {
            expected: 1,
            actual: 0,
        };
        assert!(format!("{}", e3).contains("NEXA-NXR"));

        let e4 = ResourceError::WrongType {
            expected: ResourceTypeId(1),
            actual: ResourceTypeId(2),
        };
        assert!(format!("{}", e4).contains("NEXA-NXR"));

        let e5 = ResourceError::NotOpen { handle: 5 };
        assert!(format!("{}", e5).contains("NEXA-NXR"));

        let e6 = ResourceError::AlreadyClosed { handle: 3 };
        assert!(format!("{}", e6).contains("NEXA-NXR"));

        let e7 = ResourceError::LeaseExpired { handle: 8 };
        assert!(format!("{}", e7).contains("NEXA-NXR"));

        let e8 = ResourceError::Unauthorized {
            handle: 4,
            detail: "no perms".into(),
        };
        assert!(format!("{}", e8).contains("NEXA-NXR"));
    }
}
