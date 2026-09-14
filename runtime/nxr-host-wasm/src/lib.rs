use std::error;
use std::fmt;
use std::io;

use nxr_observability::SecretRedactor;
use nxr_providers::ProviderError;

// ─── AbiError ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum AbiError {
    InvalidUtf8,
    PointerOutsideMemory { ptr: u32, len: u32 },
    PtrLenOverflow { ptr: u32, len: u32 },
    InvalidHandle(u64),
    InvalidEnumDiscriminant { value: i32 },
    BufferTooSmall { needed: usize, available: usize },
    InvalidPath(String),
    InvalidLength { field: String, value: u32 },
}

impl fmt::Display for AbiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AbiError::InvalidUtf8 => write!(f, "ABI error: invalid UTF-8"),
            AbiError::PointerOutsideMemory { ptr, len } => {
                write!(
                    f,
                    "ABI error: pointer {ptr} with length {len} outside memory bounds"
                )
            }
            AbiError::PtrLenOverflow { ptr, len } => {
                write!(
                    f,
                    "ABI error: pointer-length overflow at ptr={ptr}, len={len}"
                )
            }
            AbiError::InvalidHandle(h) => write!(f, "ABI error: invalid handle {h}"),
            AbiError::InvalidEnumDiscriminant { value } => {
                write!(f, "ABI error: invalid enum discriminant {value}")
            }
            AbiError::BufferTooSmall { needed, available } => {
                write!(
                    f,
                    "ABI error: buffer too small, need {needed} bytes, have {available}"
                )
            }
            AbiError::InvalidPath(p) => write!(f, "ABI error: invalid path: {p}"),
            AbiError::InvalidLength { field, value } => {
                write!(f, "ABI error: invalid length for field '{field}': {value}")
            }
        }
    }
}

impl error::Error for AbiError {}

impl From<AbiError> for io::Error {
    fn from(e: AbiError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, e.to_string())
    }
}

// ─── GuestMemory ──────────────────────────────────────────────

pub struct GuestMemory<'a> {
    data: &'a [u8],
}

impl<'a> GuestMemory<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    pub fn size(&self) -> u32 {
        self.data.len() as u32
    }

    fn bounds_of(&self, ptr: u32, len: u32) -> Result<(usize, usize), AbiError> {
        let start = ptr as usize;
        let end = start
            .checked_add(len as usize)
            .ok_or(AbiError::PtrLenOverflow { ptr, len })?;
        if end > self.data.len() {
            return Err(AbiError::PointerOutsideMemory { ptr, len });
        }
        Ok((start, end))
    }

    pub fn validate_range(&self, ptr: u32, len: u32) -> Result<(), AbiError> {
        self.bounds_of(ptr, len)?;
        Ok(())
    }

    pub fn read_u32(&self, ptr: u32) -> Result<u32, AbiError> {
        let (start, _end) = self.bounds_of(ptr, 4)?;
        Ok(u32::from_le_bytes([
            self.data[start],
            self.data[start + 1],
            self.data[start + 2],
            self.data[start + 3],
        ]))
    }

    pub fn read_u64(&self, ptr: u32) -> Result<u64, AbiError> {
        let (start, _end) = self.bounds_of(ptr, 8)?;
        Ok(u64::from_le_bytes([
            self.data[start],
            self.data[start + 1],
            self.data[start + 2],
            self.data[start + 3],
            self.data[start + 4],
            self.data[start + 5],
            self.data[start + 6],
            self.data[start + 7],
        ]))
    }

    pub fn read_bytes(&self, ptr: u32, len: u32) -> Result<&'a [u8], AbiError> {
        let (start, end) = self.bounds_of(ptr, len)?;
        Ok(&self.data[start..end])
    }

    pub fn read_string(&self, ptr: u32, len: u32) -> Result<String, AbiError> {
        let bytes = self.read_bytes(ptr, len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| AbiError::InvalidUtf8)
    }
}

// ─── AbiDecoder ───────────────────────────────────────────────

pub struct AbiDecoder<'a> {
    memory: &'a GuestMemory<'a>,
    offset: u32,
}

impl<'a> AbiDecoder<'a> {
    pub fn new(memory: &'a GuestMemory<'a>) -> Self {
        Self { memory, offset: 0 }
    }

    pub fn decode_i32(&mut self) -> Result<i32, AbiError> {
        let val = self.memory.read_u32(self.offset)? as i32;
        self.offset = self.offset.wrapping_add(4);
        Ok(val)
    }

    pub fn decode_u32(&mut self) -> Result<u32, AbiError> {
        let val = self.memory.read_u32(self.offset)?;
        self.offset = self.offset.wrapping_add(4);
        Ok(val)
    }

    pub fn decode_i64(&mut self) -> Result<i64, AbiError> {
        let val = self.memory.read_u64(self.offset)? as i64;
        self.offset = self.offset.wrapping_add(8);
        Ok(val)
    }

    pub fn decode_string(&mut self) -> Result<String, AbiError> {
        let ptr = self.decode_u32()? as i32;
        let len = self.decode_u32()? as i32;
        if ptr < 0 || len < 0 {
            return Err(AbiError::PointerOutsideMemory {
                ptr: ptr as u32,
                len: len as u32,
            });
        }
        self.memory.read_string(ptr as u32, len as u32)
    }

    pub fn decode_path(&mut self) -> Result<String, AbiError> {
        let s = self.decode_string()?;
        if s.contains('\0') {
            return Err(AbiError::InvalidPath(s));
        }
        Ok(s)
    }

    pub fn decode_bool(&mut self) -> Result<bool, AbiError> {
        let val = self.decode_i32()?;
        match val {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(AbiError::InvalidEnumDiscriminant { value: val }),
        }
    }
}

// ─── AbiEncoder ───────────────────────────────────────────────

pub struct AbiEncoder<'a> {
    memory: &'a mut [u8],
    offset: u32,
}

impl<'a> AbiEncoder<'a> {
    pub fn new(memory: &'a mut [u8]) -> Self {
        Self { memory, offset: 0 }
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), AbiError> {
        let start = self.offset as usize;
        let end = start + bytes.len();
        if end > self.memory.len() {
            return Err(AbiError::BufferTooSmall {
                needed: end,
                available: self.memory.len(),
            });
        }
        self.memory[start..end].copy_from_slice(bytes);
        self.offset += bytes.len() as u32;
        Ok(())
    }

    pub fn encode_i32(&mut self, value: i32) -> Result<(), AbiError> {
        self.write_bytes(&value.to_le_bytes())
    }

    pub fn encode_u32(&mut self, value: u32) -> Result<(), AbiError> {
        self.write_bytes(&value.to_le_bytes())
    }

    pub fn encode_i64(&mut self, value: i64) -> Result<(), AbiError> {
        self.write_bytes(&value.to_le_bytes())
    }

    pub fn encode_bool(&mut self, value: bool) -> Result<(), AbiError> {
        self.encode_i32(if value { 1 } else { 0 })
    }

    pub fn encode_handle(&mut self, handle: i64) -> Result<(), AbiError> {
        self.encode_i64(handle)
    }
}

// ─── ResultStatus ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ResultStatus {
    Success = 0,
    NotFound = 1,
    PermissionDenied = 2,
    InvalidHandle = 3,
    IoError = 4,
    LimitExceeded = 5,
    InvalidArgument = 6,
    Unavailable = 7,
}

impl ResultStatus {
    pub fn discriminant(&self) -> i32 {
        *self as i32
    }

    pub fn from_provider_error(error: &ProviderError) -> Self {
        match error {
            ProviderError::NotFound(_) => ResultStatus::NotFound,
            ProviderError::PermissionDenied(_) => ResultStatus::PermissionDenied,
            ProviderError::AlreadyExists(_) => ResultStatus::InvalidArgument,
            ProviderError::InvalidHandle(_) => ResultStatus::InvalidHandle,
            ProviderError::IoError(_) => ResultStatus::IoError,
            ProviderError::LimitExceeded(_) => ResultStatus::LimitExceeded,
            ProviderError::Utf8Error(_) => ResultStatus::InvalidArgument,
            ProviderError::Unavailable(_) => ResultStatus::Unavailable,
        }
    }
}

// ─── HostFunctionTable ────────────────────────────────────────

pub struct HostFunctionEntry {
    pub module: String,
    pub name: String,
    pub kind: HostFunctionKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostFunctionKind {
    ConsoleWriteUtf8,
    RuntimeTrap,
    RuntimeContractFail,
    RuntimePanic,
    Custom(String),
}

pub struct HostFunctionTable {
    entries: Vec<HostFunctionEntry>,
}

impl HostFunctionTable {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    fn register(&mut self, module: &str, name: &str, kind: HostFunctionKind) {
        self.entries.push(HostFunctionEntry {
            module: module.to_string(),
            name: name.to_string(),
            kind,
        });
    }

    pub fn register_console_write_utf8(&mut self) {
        self.register(
            "nxr",
            "console_write_utf8",
            HostFunctionKind::ConsoleWriteUtf8,
        );
    }

    pub fn register_runtime_trap(&mut self) {
        self.register("nxr", "runtime_trap", HostFunctionKind::RuntimeTrap);
    }

    pub fn register_runtime_contract_fail(&mut self) {
        self.register(
            "nxr",
            "runtime_contract_fail",
            HostFunctionKind::RuntimeContractFail,
        );
    }

    pub fn register_runtime_panic(&mut self) {
        self.register("nxr", "runtime_panic", HostFunctionKind::RuntimePanic);
    }

    pub fn entries(&self) -> &[HostFunctionEntry] {
        &self.entries
    }

    pub fn is_registered(&self, module: &str, name: &str) -> bool {
        self.entries
            .iter()
            .any(|e| e.module == module && e.name == name)
    }
}

impl Default for HostFunctionTable {
    fn default() -> Self {
        Self::new()
    }
}

// ─── HostAdapter ──────────────────────────────────────────────

pub struct HostAdapter {
    function_table: HostFunctionTable,
    redactor: SecretRedactor,
}

impl HostAdapter {
    pub fn new() -> Self {
        let mut table = HostFunctionTable::new();
        table.register_console_write_utf8();
        table.register_runtime_trap();
        table.register_runtime_contract_fail();
        table.register_runtime_panic();

        Self {
            function_table: table,
            redactor: SecretRedactor::new(),
        }
    }

    pub fn function_table(&self) -> &HostFunctionTable {
        &self.function_table
    }

    pub fn decode_console_write_utf8(
        &self,
        memory: &GuestMemory,
        ptr: i32,
        len: i32,
    ) -> Result<Vec<u8>, AbiError> {
        if ptr < 0 || len < 0 {
            return Err(AbiError::PointerOutsideMemory {
                ptr: ptr as u32,
                len: len as u32,
            });
        }
        let bytes = memory.read_bytes(ptr as u32, len as u32)?;
        Ok(bytes.to_vec())
    }

    pub fn decode_runtime_trap_args(
        &self,
        memory: &GuestMemory,
        kind: i32,
        location: i32,
    ) -> Result<(i32, i32), AbiError> {
        if kind < 0 {
            return Err(AbiError::InvalidEnumDiscriminant { value: kind });
        }
        if location < 0 {
            return Err(AbiError::PointerOutsideMemory {
                ptr: location as u32,
                len: 0,
            });
        }
        memory.validate_range(location as u32, 0)?;
        Ok((kind, location))
    }

    pub fn decode_panic_message(
        &self,
        memory: &GuestMemory,
        msg_ptr: i32,
        msg_len: i32,
    ) -> Result<String, AbiError> {
        if msg_ptr < 0 || msg_len < 0 {
            return Err(AbiError::PointerOutsideMemory {
                ptr: msg_ptr as u32,
                len: msg_len as u32,
            });
        }
        memory.read_string(msg_ptr as u32, msg_len as u32)
    }

    pub fn encode_result_status(
        &self,
        out_ptr: u32,
        status: ResultStatus,
        memory: &mut [u8],
    ) -> Result<(), AbiError> {
        let start = out_ptr as usize;
        let end = start + 4;
        if end > memory.len() {
            return Err(AbiError::BufferTooSmall {
                needed: end,
                available: memory.len(),
            });
        }
        memory[start..end].copy_from_slice(&(status.discriminant()).to_le_bytes());
        Ok(())
    }

    pub fn validate_guest_input(
        &self,
        memory: &GuestMemory,
        ptr: u32,
        len: u32,
    ) -> Result<(), AbiError> {
        memory.validate_range(ptr, len)?;
        Ok(())
    }

    pub fn redact_string(&self, input: &str) -> String {
        self.redactor.redact_string(input)
    }
}

impl Default for HostAdapter {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guest_memory_validate_range() {
        let data = [0u8; 64];
        let mem = GuestMemory::new(&data);
        assert!(mem.validate_range(0, 32).is_ok());
        assert!(mem.validate_range(32, 32).is_ok());
        assert!(mem.validate_range(33, 32).is_err());
    }

    #[test]
    fn test_guest_memory_out_of_bounds() {
        let data = [0u8; 16];
        let mem = GuestMemory::new(&data);
        assert!(mem.validate_range(0, 32).is_err());
        assert!(mem.validate_range(12, 16).is_err());
    }

    #[test]
    fn test_guest_memory_read_u32() {
        let data: [u8; 8] = [0x78, 0x56, 0x34, 0x12, 0x00, 0x00, 0x00, 0x00];
        let mem = GuestMemory::new(&data);
        assert_eq!(mem.read_u32(0).unwrap(), 0x12345678);
    }

    #[test]
    fn test_guest_memory_read_string() {
        let data = b"hello world";
        let mem = GuestMemory::new(data);
        let s = mem.read_string(0, 5).unwrap();
        assert_eq!(s, "hello");
        let s2 = mem.read_string(6, 5).unwrap();
        assert_eq!(s2, "world");
    }

    #[test]
    fn test_guest_memory_read_string_invalid_utf8() {
        let data = [0xFF, 0xFE, 0x00, 0x01];
        let mem = GuestMemory::new(&data);
        assert!(mem.read_string(0, 2).is_err());
    }

    #[test]
    fn test_abi_decoder_i32() {
        let mut buf = [0u8; 16];
        buf[0..4].copy_from_slice(&42i32.to_le_bytes());
        let mem = GuestMemory::new(&buf);
        let mut decoder = AbiDecoder::new(&mem);
        assert_eq!(decoder.decode_i32().unwrap(), 42);
    }

    #[test]
    fn test_abi_decoder_string() {
        let mut buf = [0u8; 64];
        let text = b"hello abi";
        let ptr = 16u32;
        let len = text.len() as u32;
        buf[0..4].copy_from_slice(&ptr.to_le_bytes());
        buf[4..8].copy_from_slice(&len.to_le_bytes());
        buf[ptr as usize..(ptr + len) as usize].copy_from_slice(text);

        let mem = GuestMemory::new(&buf);
        let mut decoder = AbiDecoder::new(&mem);
        assert_eq!(decoder.decode_string().unwrap(), "hello abi");
    }

    #[test]
    fn test_abi_decoder_path() {
        let mut buf = [0u8; 64];
        let path = b"/tmp/test.txt";
        let ptr = 16u32;
        let len = path.len() as u32;
        buf[0..4].copy_from_slice(&ptr.to_le_bytes());
        buf[4..8].copy_from_slice(&len.to_le_bytes());
        buf[ptr as usize..(ptr + len) as usize].copy_from_slice(path);

        let mem = GuestMemory::new(&buf);
        let mut decoder = AbiDecoder::new(&mem);
        assert_eq!(decoder.decode_path().unwrap(), "/tmp/test.txt");

        // Path with NUL should fail
        let mut buf2 = [0u8; 64];
        let bad_path = b"/tmp\0/evil";
        let ptr2 = 16u32;
        let len2 = bad_path.len() as u32;
        buf2[0..4].copy_from_slice(&ptr2.to_le_bytes());
        buf2[4..8].copy_from_slice(&len2.to_le_bytes());
        buf2[ptr2 as usize..(ptr2 + len2) as usize].copy_from_slice(bad_path);

        let mem2 = GuestMemory::new(&buf2);
        let mut decoder2 = AbiDecoder::new(&mem2);
        assert!(decoder2.decode_path().is_err());
    }

    #[test]
    fn test_abi_encoder_i32() {
        let mut buf = [0u8; 16];
        {
            let mut encoder = AbiEncoder::new(&mut buf);
            encoder.encode_i32(-99).unwrap();
            encoder.encode_i32(256).unwrap();
        }
        assert_eq!(i32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]), -99);
        assert_eq!(i32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]), 256);
    }

    #[test]
    fn test_abi_encoder_handle() {
        let mut buf = [0u8; 16];
        let handle_value: i64 = 0x000000010000002A;
        {
            let mut encoder = AbiEncoder::new(&mut buf);
            encoder.encode_handle(handle_value).unwrap();
        }
        let decoded = i64::from_le_bytes([
            buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
        ]);
        assert_eq!(decoded, handle_value);
    }

    #[test]
    fn test_abi_error_display() {
        let e1 = AbiError::InvalidUtf8;
        assert!(e1.to_string().contains("invalid UTF-8"));

        let e2 = AbiError::PointerOutsideMemory { ptr: 100, len: 50 };
        assert!(e2.to_string().contains("100"));
        assert!(e2.to_string().contains("50"));

        let e3 = AbiError::InvalidHandle(42);
        assert!(e3.to_string().contains("42"));

        let e4 = AbiError::InvalidEnumDiscriminant { value: -1 };
        assert!(e4.to_string().contains("-1"));

        let e5 = AbiError::BufferTooSmall {
            needed: 100,
            available: 10,
        };
        assert!(e5.to_string().contains("100"));
        assert!(e5.to_string().contains("10"));

        let e6 = AbiError::InvalidPath("/bad".to_string());
        assert!(e6.to_string().contains("/bad"));

        let e7 = AbiError::InvalidLength {
            field: "size".to_string(),
            value: 999,
        };
        assert!(e7.to_string().contains("size"));
        assert!(e7.to_string().contains("999"));
    }

    #[test]
    fn test_result_status_from_provider_error() {
        assert_eq!(
            ResultStatus::from_provider_error(&ProviderError::NotFound("x".into())),
            ResultStatus::NotFound
        );
        assert_eq!(
            ResultStatus::from_provider_error(&ProviderError::PermissionDenied("x".into())),
            ResultStatus::PermissionDenied
        );
        assert_eq!(
            ResultStatus::from_provider_error(&ProviderError::InvalidHandle(1)),
            ResultStatus::InvalidHandle
        );
        assert_eq!(
            ResultStatus::from_provider_error(&ProviderError::IoError("x".into())),
            ResultStatus::IoError
        );
        assert_eq!(
            ResultStatus::from_provider_error(&ProviderError::LimitExceeded("x".into())),
            ResultStatus::LimitExceeded
        );
        assert_eq!(
            ResultStatus::from_provider_error(&ProviderError::AlreadyExists("x".into())),
            ResultStatus::InvalidArgument
        );
        assert_eq!(
            ResultStatus::from_provider_error(&ProviderError::Utf8Error("x".into())),
            ResultStatus::InvalidArgument
        );
        assert_eq!(
            ResultStatus::from_provider_error(&ProviderError::Unavailable("x".into())),
            ResultStatus::Unavailable
        );
    }

    #[test]
    fn test_host_function_table_register() {
        let mut table = HostFunctionTable::new();
        assert!(table.entries().is_empty());

        table.register_console_write_utf8();
        table.register_runtime_trap();
        table.register_runtime_contract_fail();
        table.register_runtime_panic();

        assert_eq!(table.entries().len(), 4);
        assert!(table.is_registered("nxr", "console_write_utf8"));
        assert!(table.is_registered("nxr", "runtime_trap"));
        assert!(table.is_registered("nxr", "runtime_contract_fail"));
        assert!(table.is_registered("nxr", "runtime_panic"));
        assert!(!table.is_registered("nxr", "nonexistent"));
        assert!(!table.is_registered("other", "console_write_utf8"));
    }

    #[test]
    fn test_host_adapter_decode_console_write() {
        let adapter = HostAdapter::new();
        let data = b"hello from guest";
        let mem = GuestMemory::new(data);

        let result = adapter.decode_console_write_utf8(&mem, 0, 5).unwrap();
        assert_eq!(result, b"hello");

        assert!(adapter.decode_console_write_utf8(&mem, -1, 5).is_err());
    }

    #[test]
    fn test_host_adapter_decode_panic() {
        let adapter = HostAdapter::new();
        let msg = b"something went wrong";
        let mut buf = [0u8; 64];
        buf[20..20 + msg.len()].copy_from_slice(msg);
        let mem = GuestMemory::new(&buf);

        let result = adapter.decode_panic_message(&mem, 20, 20).unwrap();
        assert_eq!(result, "something went wrong");

        assert!(adapter.decode_panic_message(&mem, -1, 5).is_err());
    }

    #[test]
    fn test_host_adapter_validate_guest_input() {
        let adapter = HostAdapter::new();
        let data = [0u8; 100];
        let mem = GuestMemory::new(&data);

        assert!(adapter.validate_guest_input(&mem, 0, 50).is_ok());
        assert!(adapter.validate_guest_input(&mem, 99, 2).is_err());
    }

    #[test]
    fn test_host_adapter_redact() {
        let adapter = HostAdapter::new();
        let result = adapter.redact_string("my secret value");
        assert_eq!(result, "my secret value");
    }
}
