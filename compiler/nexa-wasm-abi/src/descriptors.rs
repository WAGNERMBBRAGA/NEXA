pub const STRING_DESCRIPTOR_BYTE_LEN_OFFSET: u32 = 0;
pub const STRING_DESCRIPTOR_FLAGS_OFFSET: u32 = 4;
pub const STRING_DESCRIPTOR_DATA_OFFSET: u32 = 8;
pub const STRING_DESCRIPTOR_SIZE: u32 = 12;

pub const BYTES_DESCRIPTOR_BYTE_LEN_OFFSET: u32 = 0;
pub const BYTES_DESCRIPTOR_FLAGS_OFFSET: u32 = 4;
pub const BYTES_DESCRIPTOR_DATA_OFFSET: u32 = 8;

pub const ARRAY_DESCRIPTOR_DATA_PTR_OFFSET: u32 = 0;
pub const ARRAY_DESCRIPTOR_LEN_OFFSET: u32 = 4;
pub const ARRAY_DESCRIPTOR_CAPACITY_OFFSET: u32 = 8;
pub const ARRAY_DESCRIPTOR_SIZE: u32 = 12;
pub const ARRAY_DESCRIPTOR_ALIGN: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum StringFlag {
    Static = 0,
    Owned = 1,
}

impl StringFlag {
    pub fn to_i32(self) -> i32 {
        self as i32
    }

    pub fn from_i32(v: i32) -> Option<Self> {
        match v {
            0 => Some(Self::Static),
            1 => Some(Self::Owned),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StringDescriptor {
    pub byte_len: u32,
    pub flag: StringFlag,
}

impl StringDescriptor {
    pub fn new_static(byte_len: u32) -> Self {
        Self {
            byte_len,
            flag: StringFlag::Static,
        }
    }

    pub fn new_owned(byte_len: u32) -> Self {
        Self {
            byte_len,
            flag: StringFlag::Owned,
        }
    }

    pub fn total_size(&self) -> u32 {
        STRING_DESCRIPTOR_DATA_OFFSET + self.byte_len
    }
}

#[derive(Debug, Clone)]
pub struct ArrayDescriptor {
    pub data_ptr: u32,
    pub len: u32,
    pub capacity: u32,
}

impl ArrayDescriptor {
    pub fn new(data_ptr: u32, len: u32, capacity: u32) -> Self {
        Self {
            data_ptr,
            len,
            capacity,
        }
    }

    pub fn total_size() -> u32 {
        ARRAY_DESCRIPTOR_SIZE
    }
}

pub fn compute_string_descriptor_static_offset(
    static_data_offset: u32,
    byte_len: u32,
) -> StringDescriptor {
    let _ = static_data_offset;
    StringDescriptor::new_static(byte_len)
}

pub fn is_static_string(flag: StringFlag) -> bool {
    matches!(flag, StringFlag::Static)
}

pub fn is_owned_string(flag: StringFlag) -> bool {
    matches!(flag, StringFlag::Owned)
}

pub fn string_data_ptr(descriptor_ptr: u32) -> u32 {
    descriptor_ptr + STRING_DESCRIPTOR_DATA_OFFSET
}

pub fn string_byte_len(descriptor_ptr: u32, memory: &[u8]) -> Option<u32> {
    let offset = descriptor_ptr as usize;
    if offset + 4 > memory.len() {
        return None;
    }
    let bytes = [
        memory[offset],
        memory[offset + 1],
        memory[offset + 2],
        memory[offset + 3],
    ];
    Some(u32::from_le_bytes(bytes))
}

pub fn string_flag_from_memory(descriptor_ptr: u32, memory: &[u8]) -> Option<StringFlag> {
    let offset = (descriptor_ptr + STRING_DESCRIPTOR_FLAGS_OFFSET) as usize;
    if offset + 4 > memory.len() {
        return None;
    }
    let bytes = [
        memory[offset],
        memory[offset + 1],
        memory[offset + 2],
        memory[offset + 3],
    ];
    StringFlag::from_i32(i32::from_le_bytes(bytes))
}
