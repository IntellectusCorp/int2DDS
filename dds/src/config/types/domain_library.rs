use serde::{de, Deserialize, Deserializer, Serialize};

use crate::config::types::entity_qos::TopicQos;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct DomainLibrary {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) domains: Vec<Domain>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Domain {
    pub(crate) name: String,
    #[serde(default, deserialize_with = "deserialize_opt_i32")]
    pub(crate) domain_id: Option<i32>,
    #[serde(default)]
    pub(crate) register_types: Vec<RegisterType>,
    #[serde(default)]
    pub(crate) topics: Vec<TopicDecl>,
}

// XML attributes arrive as strings; accept both string and integer forms.
fn deserialize_opt_i32<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum IntOrString {
        Int(i32),
        Str(String),
    }

    match Option::<IntOrString>::deserialize(deserializer)? {
        None => Ok(None),
        Some(IntOrString::Int(v)) => Ok(Some(v)),
        Some(IntOrString::Str(s)) => s.parse::<i32>().map(Some).map_err(de::Error::custom),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RegisterType {
    pub(crate) name: String,
    pub(crate) type_ref: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct TopicDecl {
    pub(crate) name: String,
    pub(crate) register_type_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) topic_qos: Option<TopicQos>,
}
