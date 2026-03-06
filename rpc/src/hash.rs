//! HASH function implementation (7.5.1.1.2)
//!
//! Converts the first 4 bytes of an MD5 hash into a little-endian 32-bit integer.
//! Used to generate hash identifiers for operation names, exception type names, etc.

pub(crate) fn rpc_hash(name: &str) -> i32 {
    let digest = md5::compute(name.as_bytes());
    let bytes = digest.0;
    i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_deterministic() {
        assert_eq!(rpc_hash("setSpeed"), rpc_hash("setSpeed"));
    }

    #[test]
    fn hash_different_names() {
        assert_ne!(rpc_hash("setSpeed"), rpc_hash("getSpeed"));
    }

    #[test]
    fn hash_empty_string() {
        // MD5("") = d41d8cd98f00b204e9800998ecf8427e
        // First 4 bytes: 0xd4, 0x1d, 0x8c, 0xd9 -> little-endian i32
        let h = rpc_hash("");
        let expected = i32::from_le_bytes([0xd4, 0x1d, 0x8c, 0xd9]);
        assert_eq!(h, expected);
    }
}
