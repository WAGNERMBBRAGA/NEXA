pub const TARGET_IDENTITY: &str = "wasm32-nexa";
pub const ABI_VERSION: u32 = 1;
pub const LANGUAGE_PROFILE_VERSION: &str = "1.0";
pub const ABI_CUSTOM_SECTION: &str = "nexa.abi";
pub const SOURCE_MAP_CUSTOM_SECTION: &str = "nexa.source_map";
pub const MEMORY_EXPORT_NAME: &str = "memory";
pub const ENTRYPOINT_EXPORT_NAME: &str = "__nexa_main";
pub const RUNTIME_IMPORT_MODULE: &str = "nexa";

pub const NULL_POINTER_SENTINEL: u32 = 0;
pub const HEAP_START_ALIGNMENT: u32 = 16;

pub const STACK_POINTER_GLOBAL: &str = "__nexa_sp";
pub const BUMP_HEAP_CURSOR_GLOBAL: &str = "__nexa_heap_cursor";
pub const ALLOC_FN_NAME: &str = "__nexa_alloc";
pub const FREE_FN_NAME: &str = "__nexa_free";
pub const MEMCPY_FN_NAME: &str = "__nexa_memcpy";
pub const TRAP_FN_NAME: &str = "__nexa_trap";

pub const MAGIC: &[u8; 4] = b"NEXA";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TargetIdentity;

impl TargetIdentity {
    pub const WASM32_NEXA: &'static str = TARGET_IDENTITY;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildProfile {
    Debug,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    Application,
    Library,
}
