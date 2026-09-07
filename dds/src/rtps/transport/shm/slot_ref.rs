//! Fixed 20-byte descriptor that replaces SerializedPayload on the SHM path.

pub(crate) const SLOT_REF_LEN: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SlotRef {
    pub(crate) owner_slot: u16,
    pub(crate) owner_epoch_low: u16,
    pub(crate) class: u16,
    pub(crate) index: u32,
    pub(crate) generation: u32,
    pub(crate) len: u32,
}

impl SlotRef {
    pub(crate) fn encode(&self) -> [u8; SLOT_REF_LEN] {
        let mut out = [0u8; SLOT_REF_LEN];
        out[0..2].copy_from_slice(&self.owner_slot.to_le_bytes());
        out[2..4].copy_from_slice(&self.owner_epoch_low.to_le_bytes());
        out[4..6].copy_from_slice(&self.class.to_le_bytes());
        // out[6..8] reserved
        out[8..12].copy_from_slice(&self.index.to_le_bytes());
        out[12..16].copy_from_slice(&self.generation.to_le_bytes());
        out[16..20].copy_from_slice(&self.len.to_le_bytes());
        out
    }

    pub(crate) fn decode(bytes: &[u8]) -> Option<SlotRef> {
        if bytes.len() < SLOT_REF_LEN {
            return None;
        }
        Some(SlotRef {
            owner_slot: u16::from_le_bytes(bytes[0..2].try_into().ok()?),
            owner_epoch_low: u16::from_le_bytes(bytes[2..4].try_into().ok()?),
            class: u16::from_le_bytes(bytes[4..6].try_into().ok()?),
            index: u32::from_le_bytes(bytes[8..12].try_into().ok()?),
            generation: u32::from_le_bytes(bytes[12..16].try_into().ok()?),
            len: u32::from_le_bytes(bytes[16..20].try_into().ok()?),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SlotRef {
        SlotRef {
            owner_slot: 7,
            owner_epoch_low: 0xBEEF,
            class: 2,
            index: 11,
            generation: 5,
            len: 4096,
        }
    }

    #[test]
    fn round_trips() {
        let bytes = sample().encode();
        assert_eq!(bytes.len(), SLOT_REF_LEN);
        assert_eq!(SlotRef::decode(&bytes), Some(sample()));
    }

    #[test]
    fn decode_rejects_short_input() {
        let bytes = sample().encode();
        assert_eq!(SlotRef::decode(&bytes[..SLOT_REF_LEN - 1]), None);
    }

    #[test]
    fn decode_ignores_trailing_bytes() {
        let mut bytes = sample().encode().to_vec();
        bytes.extend_from_slice(&[0xAA; 8]);
        assert_eq!(SlotRef::decode(&bytes), Some(sample()));
    }
}
