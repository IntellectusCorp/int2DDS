//! ABI identity, for a binding that has already loaded the library and wants to know
//! what it got.

/// One `CARGO_PKG_VERSION_*` component. `str::parse` is not const, so the digits are
/// walked by hand; the components are decimal and set by Cargo, so there is nothing
/// to reject. Private, so cbindgen does not try to render the packing into the header.
const fn component(s: &str) -> u32 {
    let digits = s.as_bytes();
    let mut value = 0;
    let mut i = 0;
    while i < digits.len() {
        value = value * 10 + (digits[i] - b'0') as u32;
        i += 1;
    }
    value
}

const MAJOR: u32 = component(env!("CARGO_PKG_VERSION_MAJOR"));
const MINOR: u32 = component(env!("CARGO_PKG_VERSION_MINOR"));
const PATCH: u32 = component(env!("CARGO_PKG_VERSION_PATCH"));

const _: () = assert!(
    MAJOR < 256 && MINOR < 256 && PATCH < 256,
    "a version component no longer fits the byte it is packed into. Widen the packing \
     in int2dds_abi_version and say so in its documentation — silently truncating would \
     make two different releases report the same ABI version"
);

/// This library's ABI version, packed as `0x00MMmmpp`: major in bits 16-23, minor in
/// 8-15, patch in 0-7, bits 24-31 zero.
///
/// The value is the crate version, which the workspace declares once and everything
/// else derives from — the C# binding reads it from `Cargo.toml`, and `ffi/build.rs`
/// builds the ELF soname and the Windows VERSIONINFO out of it. So this is not a
/// second version to maintain; it is the same one, readable at runtime.
///
/// Which part is the compatibility boundary depends on where the version is. Post-1.0
/// it is `major` alone, which is why the soname carries the major only. While the
/// version stays `0.x` a **minor** bump can break the ABI as well, so until then a
/// binding must compare `major.minor` — comparing only the major would accept a
/// library it cannot call. `ffi/build.rs` records the same caveat for the soname;
/// the two are the same rule and should stay reconciled.
#[no_mangle]
pub extern "C" fn int2dds_abi_version() -> u32 {
    (MAJOR << 16) | (MINOR << 8) | PATCH
}

/// The optional capabilities of this build, as a bit set. **Every bit is reserved and
/// reads 0.**
///
/// That is the contract, not a stub waiting to be filled in. A bit belongs here only
/// once some build can clear it, and nothing in this workspace is conditionally
/// compiled: the `ffi` crate declares no features, and the one `dds` declares
/// (`factory-hook`) is not forwarded through here. Bits for surfaces that are always
/// present would be a word of constant ones — it would tell a caller nothing, and the
/// list of names behind it would drift out of step with the code with nothing to
/// notice.
///
/// The export earns its place through the direction it fails in. An unknown bit reads
/// 0, so a binding built against a newer library than the one it loaded can gate on a
/// bit this build has never heard of and take the unsupported branch, instead of
/// calling an entry point that is not there and faulting at the call site.
#[no_mangle]
pub extern "C" fn int2dds_abi_capabilities() -> u64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The const packing against a runtime parse of the same environment: the only way
    /// the reported version can diverge from the crate's is an error in the packing.
    #[test]
    fn the_version_packs_the_crate_version() {
        let version = int2dds_abi_version();
        let field = |s: &str| s.parse::<u32>().expect("Cargo sets a decimal component");
        assert_eq!((version >> 16) & 0xff, field(env!("CARGO_PKG_VERSION_MAJOR")));
        assert_eq!((version >> 8) & 0xff, field(env!("CARGO_PKG_VERSION_MINOR")));
        assert_eq!(version & 0xff, field(env!("CARGO_PKG_VERSION_PATCH")));
        assert_eq!(version >> 24, 0, "the high byte is unassigned");
    }

    #[test]
    fn every_capability_bit_is_reserved() {
        assert_eq!(
            int2dds_abi_capabilities(),
            0,
            "a capability bit was assigned. Add it here with the build configuration that \
             clears it — a bit no build can clear reports nothing and cannot be verified"
        );
    }
}
