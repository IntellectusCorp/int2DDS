//! Environment-driven SHM configuration. A bad value never fails a
//! participant: it is logged once and replaced by the default.

use log::warn;

use crate::rtps::transport::shm::pool::MAX_CLASSES;

/// 4 MiB + 12 MiB + 16 MiB = 32 MiB per participant.
pub(crate) const DEFAULT_CLASSES: [(u32, u32); 3] = [(65536, 64), (1048576, 12), (8388608, 2)];
pub(crate) const DEFAULT_RING_ENTRIES: u32 = 1024;

pub(crate) struct ShmConfig {
    pub(crate) classes: Vec<(u32, u32)>,
    pub(crate) ring_entries: u32,
    pub(crate) enabled: bool,
}

impl ShmConfig {
    pub(crate) fn from_env() -> ShmConfig {
        let classes = match std::env::var("INT2DDS_SHM_POOL_CLASSES").ok().filter(|s| !s.is_empty())
        {
            Some(raw) => parse_classes(&raw).unwrap_or_else(|| {
                warn!("[shm] INT2DDS_SHM_POOL_CLASSES is not usable, falling back to defaults");
                DEFAULT_CLASSES.to_vec()
            }),
            None => DEFAULT_CLASSES.to_vec(),
        };
        let ring_entries =
            match std::env::var("INT2DDS_SHM_RING_ENTRIES").ok().filter(|s| !s.is_empty()) {
                Some(raw) => parse_ring_entries(&raw).unwrap_or_else(|| {
                    warn!("[shm] INT2DDS_SHM_RING_ENTRIES is not usable, falling back to default");
                    DEFAULT_RING_ENTRIES
                }),
                None => DEFAULT_RING_ENTRIES,
            };
        let enabled = match std::env::var("INT2DDS_SHM_ZERO_COPY").ok().filter(|s| !s.is_empty()) {
            Some(raw) => !(raw.eq_ignore_ascii_case("false") || raw == "0"),
            None => true,
        };
        ShmConfig { classes, ring_entries, enabled }
    }
}

/// `size:count` entries, comma separated, sizes strictly ascending.
fn parse_classes(raw: &str) -> Option<Vec<(u32, u32)>> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for entry in raw.split(',') {
        let (size, count) = entry.split_once(':')?;
        let size: u32 = size.trim().parse().ok()?;
        let count: u32 = count.trim().parse().ok()?;
        if size == 0 || count == 0 {
            return None;
        }
        if out.last().is_some_and(|(prev, _)| *prev >= size) {
            return None;
        }
        out.push((size, count));
    }
    (!out.is_empty() && out.len() <= MAX_CLASSES).then_some(out)
}

fn parse_ring_entries(raw: &str) -> Option<u32> {
    let value: u32 = raw.trim().parse().ok()?;
    (value >= 2 && value.is_power_of_two()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_valid_class_list() {
        assert_eq!(parse_classes("65536:64,1048576:12"), Some(vec![(65536, 64), (1048576, 12)]));
    }

    #[test]
    fn rejects_non_ascending_sizes() {
        assert_eq!(parse_classes("1048576:12,65536:64"), None);
        assert_eq!(parse_classes("65536:64,65536:12"), None);
    }

    #[test]
    fn rejects_zero_count_or_size() {
        assert_eq!(parse_classes("65536:0"), None);
        assert_eq!(parse_classes("0:64"), None);
    }

    #[test]
    fn rejects_too_many_classes() {
        assert_eq!(parse_classes("1:1,2:1,3:1,4:1,5:1"), None);
        assert!(parse_classes("1:1,2:1,3:1,4:1").is_some(), "MAX_CLASSES entries fit");
    }

    #[test]
    fn rejects_malformed_entries() {
        assert_eq!(parse_classes(""), None);
        assert_eq!(parse_classes("65536"), None);
        assert_eq!(parse_classes("65536:64:7"), None);
        assert_eq!(parse_classes("abc:64"), None);
    }

    #[test]
    fn ring_entries_must_be_a_power_of_two_above_one() {
        assert_eq!(parse_ring_entries("1024"), Some(1024));
        assert_eq!(parse_ring_entries("2"), Some(2));
        assert_eq!(parse_ring_entries("1000"), None);
        assert_eq!(parse_ring_entries("1"), None);
        assert_eq!(parse_ring_entries("0"), None);
        assert_eq!(parse_ring_entries("x"), None);
    }

    #[test]
    fn defaults_are_self_consistent() {
        let raw: Vec<String> = DEFAULT_CLASSES.iter().map(|(s, c)| format!("{s}:{c}")).collect();
        assert_eq!(parse_classes(&raw.join(",")), Some(DEFAULT_CLASSES.to_vec()));
        assert_eq!(
            parse_ring_entries(&DEFAULT_RING_ENTRIES.to_string()),
            Some(DEFAULT_RING_ENTRIES)
        );
    }
}
