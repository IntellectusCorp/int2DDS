use crate::core::time;
use serde::de;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Deserialize, Serialize)]
pub(crate) struct Duration {
    #[serde(deserialize_with = "deserialize_duration_field")]
    #[serde(serialize_with = "serialize_duration_sec")]
    pub sec: i32,

    #[serde(deserialize_with = "deserialize_duration_field_u32")]
    #[serde(serialize_with = "serialize_duration_nsec")]
    pub nanosec: u32,
}

// i32 필드 역직렬화 (정수 또는 "DURATION_INFINITY_SEC" 허용)
fn deserialize_duration_field<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum IntOrString {
        Int(i32),
        String(String),
    }

    match IntOrString::deserialize(deserializer)? {
        IntOrString::Int(v) => Ok(v),
        IntOrString::String(s) => match s.as_str() {
            "DURATION_INFINITY" | "DURATION_INFINITE_SEC" => Ok(time::Duration::INFINITE_SEC),
            "DURATION_ZERO_SEC" => Ok(time::Duration::ZERO_SEC),
            _ => Err(de::Error::custom(format!("invalid duration constant: {}", s))),
        },
    }
}

// u32 필드 역직렬화
fn deserialize_duration_field_u32<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum IntOrString {
        Int(u32),
        String(String),
    }

    match IntOrString::deserialize(deserializer)? {
        IntOrString::Int(v) => Ok(v),
        IntOrString::String(s) => match s.as_str() {
            "DURATION_INFINITY" | "DURATION_INFINITE_NSEC" => Ok(time::Duration::INFINITE_NSEC),
            "DURATION_ZERO_NSEC" => Ok(time::Duration::ZERO_NSEC),
            _ => Err(de::Error::custom(format!("invalid duration constant: {}", s))),
        },
    }
}

// 직렬화 (무한 값은 상수 문자열로, 일반 값은 숫자로)
fn serialize_duration_sec<S>(sec: &i32, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if *sec == time::Duration::INFINITE_SEC {
        serializer.serialize_str("DURATION_INFINITE_SEC")
    } else {
        serializer.serialize_i32(*sec)
    }
}

fn serialize_duration_nsec<S>(nanosec: &u32, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if *nanosec == time::Duration::INFINITE_NSEC {
        serializer.serialize_str("DURATION_INFINITE_NSEC")
    } else {
        serializer.serialize_u32(*nanosec)
    }
}

// 변환
impl From<Duration> for time::Duration {
    fn from(external: Duration) -> Self {
        Self { sec: external.sec, nanosec: external.nanosec }
    }
}

impl From<time::Duration> for Duration {
    fn from(internal: time::Duration) -> Self {
        Self { sec: internal.sec, nanosec: internal.nanosec }
    }
}
