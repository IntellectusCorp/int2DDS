//! OMG-style `<qos_library>` XML loading.
//!
//! Converts the XML to a [`serde_json::Value`] and reuses the JSON [`QosLibrary`]
//! deserialization, so path resolution, `base_name` inheritance and DCPS conversion
//! are shared with the JSON provider.

use std::collections::HashMap;

use roxmltree::{Document, Node};
use serde_json::{Map, Number, Value};

use crate::config::types::qos_profile::QosLibrary;
use crate::dcps::core::error::{DdsError, DdsResult};

// Entity-QoS containers: a `name` attribute makes them named sequence entries.
const ENTITY_QOS_KEYS: &[&str] = &[
    "domain_participant_qos",
    "topic_qos",
    "publisher_qos",
    "subscriber_qos",
    "datawriter_qos",
    "datareader_qos",
];

/// Parses `<qos_library>` definitions from a `<dds>` root (other sections ignored)
/// or a standalone `<qos_library>` root.
pub(crate) fn parse_qos_libraries(xml: &str) -> DdsResult<Vec<QosLibrary>> {
    let doc =
        Document::parse(xml).map_err(|e| DdsError::Error(format!("XML QoS: parse error: {e}")))?;
    let root = doc.root_element();
    let lib_nodes: Vec<Node> = match root.tag_name().name() {
        "dds" => root
            .children()
            .filter(Node::is_element)
            .filter(|n| n.tag_name().name() == "qos_library")
            .collect(),
        "qos_library" => vec![root],
        other => {
            return Err(DdsError::Error(format!(
                "XML QoS: expected <dds> or <qos_library> root, found <{other}>"
            )))
        }
    };

    lib_nodes
        .into_iter()
        .map(|node| {
            serde_json::from_value(element_to_value(node))
                .map_err(|e| DdsError::Error(format!("XML QoS: invalid qos_library: {e}")))
        })
        .collect()
}

fn rename_key(tag: &str) -> &str {
    if tag == "qos_profile" {
        "qos_profiles"
    } else {
        tag
    }
}

fn element_to_value(node: Node) -> Value {
    let tag = node.tag_name().name();
    let mut map = Map::new();

    for attr in node.attributes() {
        // `is_default_profile` is the only boolean attribute; identifiers stay strings.
        let value = if attr.name() == "is_default_profile" {
            infer_scalar(attr.value())
        } else {
            Value::String(attr.value().to_string())
        };
        map.insert(attr.name().to_string(), value);
    }

    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<Node>> = HashMap::new();
    for child in node.children().filter(Node::is_element) {
        let key = rename_key(child.tag_name().name()).to_string();
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(child);
    }

    for key in order {
        let nodes = &groups[&key];
        let any_named = nodes.iter().any(|n| n.has_attribute("name"));
        let mut values: Vec<Value> = nodes.iter().map(|n| child_value(tag, &key, *n)).collect();
        let value = if is_array(tag, &key, nodes.len(), any_named) {
            Value::Array(values)
        } else {
            values.pop().unwrap_or(Value::Null)
        };
        map.insert(key, value);
    }

    Value::Object(map)
}

fn child_value(parent_tag: &str, key: &str, node: Node) -> Value {
    if node.children().any(|c| c.is_element()) || node.attributes().next().is_some() {
        element_to_value(node)
    } else {
        leaf_value(parent_tag, key, node.text().unwrap_or("").trim())
    }
}

// Same-named siblings, profiles, and string/object sequences become JSON arrays.
fn is_array(parent_tag: &str, key: &str, count: usize, any_named: bool) -> bool {
    if count > 1 {
        return true;
    }
    match key {
        "qos_profiles" | "element" => true,
        "value" => matches!(parent_tag, "data_representation" | "property"),
        k if ENTITY_QOS_KEYS.contains(&k) => any_named,
        _ => false,
    }
}

fn leaf_value(parent_tag: &str, key: &str, text: &str) -> Value {
    if is_string_leaf(parent_tag, key) {
        Value::String(text.to_string())
    } else {
        infer_scalar(text)
    }
}

// `<value>` is a string under data parents but an integer under priority/strength;
// `parent_tag == "value"` is a property entry.
fn is_string_leaf(parent_tag: &str, key: &str) -> bool {
    match key {
        "element" | "name" | "base_name" | "topic_filter" => true,
        "value" => matches!(
            parent_tag,
            "user_data" | "group_data" | "topic_data" | "data_representation" | "value"
        ),
        _ => false,
    }
}

// Typed leaf so numeric/boolean fields deserialize; enum/constant tokens stay strings.
fn infer_scalar(text: &str) -> Value {
    let trimmed = text.trim();
    if let Ok(i) = trimmed.parse::<i64>() {
        return Value::Number(i.into());
    }
    if let Ok(f) = trimmed.parse::<f64>() {
        if let Some(n) = Number::from_f64(f) {
            return Value::Number(n);
        }
    }
    match trimmed.to_ascii_lowercase().as_str() {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => Value::String(text.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use crate::config::json::QosProvider;
    use crate::infrastructure::qos_policy::{HistoryQosPolicyKind, ReliabilityQosPolicyKind};

    fn provider(xml: &str) -> QosProvider {
        let mut p = QosProvider::new();
        p.load_xml(xml).unwrap();
        p
    }

    #[test]
    fn parses_kinds_and_numeric_depth() {
        let xml = r#"<dds><qos_library name="L"><qos_profile name="P">
            <datawriter_qos>
                <reliability><kind>RELIABLE_RELIABILITY_QOS</kind></reliability>
                <history><kind>KEEP_LAST_HISTORY_QOS</kind><depth>10</depth></history>
            </datawriter_qos>
        </qos_profile></qos_library></dds>"#;
        let qos = provider(xml).get_datawriter_qos("L::P").unwrap();
        assert_eq!(qos.reliability.kind, ReliabilityQosPolicyKind::Reliable);
        assert!(matches!(qos.history.kind, HistoryQosPolicyKind::KeepLast(10)));
    }

    #[test]
    fn named_seq_flattens_and_keeps_numbers() {
        let xml = r#"<qos_library name="L"><qos_profile name="P">
            <datawriter_qos name="W">
                <history><kind>KEEP_LAST_HISTORY_QOS</kind><depth>7</depth></history>
            </datawriter_qos>
        </qos_profile></qos_library>"#;
        let qos = provider(xml).get_datawriter_qos("L::P::W").unwrap();
        assert!(matches!(qos.history.kind, HistoryQosPolicyKind::KeepLast(7)));
    }

    #[test]
    fn partition_elements_and_default_profile() {
        let xml = r#"<dds><qos_library name="L"><qos_profile name="P" is_default_profile="true">
            <publisher_qos>
                <partition><name><element>a</element><element>b</element></name></partition>
            </publisher_qos>
        </qos_profile></qos_library></dds>"#;
        let p = provider(xml);
        assert_eq!(p.default_profile_path().as_deref(), Some("L::P"));
        let qos = p.get_publisher_qos("L::P").unwrap();
        assert_eq!(qos.partition.name, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn base_name_inheritance() {
        let xml = r#"<qos_library name="L"><qos_profile name="P">
            <datawriter_qos name="Base">
                <reliability><kind>RELIABLE_RELIABILITY_QOS</kind></reliability>
            </datawriter_qos>
            <datawriter_qos name="Derived" base_name="Base">
                <history><kind>KEEP_LAST_HISTORY_QOS</kind><depth>5</depth></history>
            </datawriter_qos>
        </qos_profile></qos_library>"#;
        let qos = provider(xml).get_datawriter_qos("L::P::Derived").unwrap();
        assert_eq!(qos.reliability.kind, ReliabilityQosPolicyKind::Reliable);
        assert!(matches!(qos.history.kind, HistoryQosPolicyKind::KeepLast(5)));
    }

    #[test]
    fn dds_root_ignores_type_sections() {
        let xml = r#"<dds>
            <types><struct name="Foo"><member name="x" type="int32"/></struct></types>
            <qos_library name="L"><qos_profile name="P">
                <datawriter_qos><reliability><kind>RELIABLE_RELIABILITY_QOS</kind></reliability></datawriter_qos>
            </qos_profile></qos_library>
        </dds>"#;
        let qos = provider(xml).get_datawriter_qos("L::P").unwrap();
        assert_eq!(qos.reliability.kind, ReliabilityQosPolicyKind::Reliable);
    }
}
