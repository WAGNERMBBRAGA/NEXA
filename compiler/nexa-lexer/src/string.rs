//! Validação lexical de escapes e literais de string/char.
//!
//! O lexer **valida** escapes mas **não decodifica/aloja** o valor: o token
//! guarda apenas o span; a decodificação semântica acontece em fase posterior.

/// Retorna `(válido, end_exclusivo)` de um escape iniciado em `pos` (que
/// deve apontar para `\`).
///
/// Escapes válidos (baseline Implementação 01):
/// `\\`, `\"`, `\n`, `\r`, `\t`, `\0`, `\u{...}`.
///
/// `\u{...}`: 1..=6 dígitos hex, valor ≤ `0x10FFFF`, não surrogate.
///
/// Em caso inválido, `end` consome o máximo possível para o scanner
/// continuar (≥ 1 byte) sem loop infinito.
pub fn scan_escape(bytes: &[u8], pos: usize) -> (bool, usize) {
    debug_assert_eq!(
        bytes.get(pos),
        Some(&b'\\'),
        "scan_escape must start at '\\'"
    );
    let Some(&c) = bytes.get(pos + 1) else {
        return (false, pos + 1);
    };
    match c {
        b'\\' | b'"' | b'n' | b'r' | b't' | b'0' => (true, pos + 2),
        b'u' => scan_unicode_escape(bytes, pos),
        _ => (false, (pos + 2).min(bytes.len())),
    }
}

fn scan_unicode_escape(bytes: &[u8], pos: usize) -> (bool, usize) {
    // bytes[pos] == '\\', bytes[pos+1] == 'u', bytes[pos+2] deve ser '{'
    if bytes.get(pos + 2) != Some(&b'{') {
        return (false, (pos + 3).min(bytes.len()));
    }
    let mut i = pos + 3;
    let mut value: u32 = 0;
    let mut digit_count: usize = 0;
    while i < bytes.len() && digit_count < 6 {
        match hex_value(bytes[i]) {
            Some(d) => {
                value = value * 16 + d;
                digit_count += 1;
                i += 1;
            }
            None => break,
        }
    }
    let closed = bytes.get(i) == Some(&b'}');
    if digit_count == 0 || !closed {
        return (false, i.min(bytes.len()));
    }
    if value > 0x10FFFF || (0xD800..=0xDFFF).contains(&value) {
        return (false, i + 1); // inclui o '}' no consumo
    }
    (true, i + 1)
}

fn hex_value(b: u8) -> Option<u32> {
    match b {
        b'0'..=b'9' => Some((b - b'0') as u32),
        b'a'..=b'f' => Some((b - b'a' + 10) as u32),
        b'A'..=b'F' => Some((b - b'A' + 10) as u32),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::scan_escape;

    fn check(input: &str) -> (bool, usize) {
        scan_escape(input.as_bytes(), 0)
    }

    #[test]
    fn valid_escapes() {
        for s in ["\\\\", "\\\"", "\\n", "\\r", "\\t", "\\0"] {
            assert_eq!(check(s), (true, 2), "{s:?}");
        }
        assert_eq!(check("\\u{1F600}"), (true, 9));
        assert_eq!(check("\\u{0}"), (true, 5));
        assert_eq!(check("\\u{10FFFF}"), (true, 10));
    }

    #[test]
    fn invalid_escapes() {
        assert_eq!(check("\\q"), (false, 2));
        assert_eq!(check("\\$"), (false, 2)); // sem \$ no baseline
        assert_eq!(check("\\u{}"), (false, 3));
        assert_eq!(check("\\u{110000}"), (false, 10));
        assert_eq!(check("\\u{D800}"), (false, 8));
        assert_eq!(check("\\u{12"), (false, 5));
        assert_eq!(check("\\u{GGG}"), (false, 3));
        assert_eq!(check("\\u{1234567}"), (false, 9));
        assert_eq!(check("\\"), (false, 1)); // '\\' sozinho no fim
    }
}
