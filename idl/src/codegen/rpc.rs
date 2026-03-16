//! RPC type code generation (7.5.1.1)
//!
//! Generates Rust types from IDL interface definitions:
//! In/Out structs, Result/Call/Return unions, Request/Reply wrappers.

/// HASH function (7.5.1.1.2)
///
/// Computes a 32-bit hash from the first 4 bytes of an MD5 digest (little-endian).
/// Used to generate discriminant values for Call/Return unions (operation hashes)
/// and Result unions (exception hashes).
pub fn rpc_hash(name: &str) -> i32 {
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
        let h = rpc_hash("");
        let expected = i32::from_le_bytes([0xd4, 0x1d, 0x8c, 0xd9]);
        assert_eq!(h, expected);
    }

    #[test]
    fn hash_uses_unqualified_name() {
        // (7.5.1.1.6) operation hash uses unqualified name
        let h1 = rpc_hash("command");
        let h2 = rpc_hash("command");
        assert_eq!(h1, h2);
        assert_ne!(rpc_hash("command"), rpc_hash("setSpeed"));
    }
}
