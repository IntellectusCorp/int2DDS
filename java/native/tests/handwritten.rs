// Tests the pure collection logic. The JNI wrapper is exercised end-to-end by
// the Java-side test added on the feature/java-core-api branch, which is the
// first branch with a DomainParticipant to build a QoS from.
use int2dds_java::handwritten::flatten_pairs;

#[test]
fn pairs_flatten_to_alternating_name_value_entries() {
    let pairs = vec![
        ("int2dds.transport.UDPv4.multicast_ttl".to_string(), "32".to_string()),
        ("int2dds.transport.UDPv4.buffer".to_string(), "1048576".to_string()),
    ];
    let flat = flatten_pairs(&pairs);
    assert_eq!(flat.len(), 4);
    assert_eq!(flat[0], b"int2dds.transport.UDPv4.multicast_ttl");
    assert_eq!(flat[1], b"32");
    assert_eq!(flat[2], b"int2dds.transport.UDPv4.buffer");
    assert_eq!(flat[3], b"1048576");
}

#[test]
fn empty_input_flattens_to_empty_output() {
    assert!(flatten_pairs(&[]).is_empty());
}

#[test]
fn non_ascii_values_survive_as_utf8() {
    let pairs = vec![("이름".to_string(), "값🌡".to_string())];
    let flat = flatten_pairs(&pairs);
    assert_eq!(flat[0], "이름".as_bytes());
    assert_eq!(flat[1], "값🌡".as_bytes());
}

#[test]
fn an_empty_value_still_occupies_its_slot() {
    // The array is positional: dropping an empty value would shift every
    // following name into a value slot.
    let pairs = vec![("a".to_string(), String::new()), ("b".to_string(), "2".to_string())];
    let flat = flatten_pairs(&pairs);
    assert_eq!(flat.len(), 4);
    assert_eq!(flat[1], b"");
    assert_eq!(flat[2], b"b");
}
