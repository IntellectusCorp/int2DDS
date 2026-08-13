use std::error::Error as StdError;

#[derive(Debug, Clone, PartialEq)]
pub enum SerializationError {
    InsufficientData,
    InvalidEncapsulation(u16),
    InvalidString,
    InvalidCharacter,
    InvalidWideCharacter,
    SerializationError(String),
    DeserializationError(String),
    // XCDR specific errors
    InvalidExtensibility,
    InvalidMemberHeader,
    TypeHashMismatch([u8; 14], [u8; 14]),
    // Enum/Union specific errors
    InvalidEnumDiscriminant(i32),
    InvalidUnionDiscriminant(i32),
    // Slice conversion error
    SliceConversionError,
    InvalidMemberId(u32),
}

impl std::fmt::Display for SerializationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SerializationError::InsufficientData => write!(f, "Insufficient data for operation"),
            SerializationError::InvalidEncapsulation(id) => {
                write!(f, "Invalid encapsulation ID: 0x{:04X}", id)
            }
            SerializationError::InvalidString => write!(f, "Invalid UTF-8 string in data"),
            SerializationError::SerializationError(msg) => {
                write!(f, "Serialization error: {}", msg)
            }
            SerializationError::InvalidCharacter => write!(f, "Invalid character in data"),
            SerializationError::InvalidWideCharacter => write!(f, "Invalid wide character in data"),
            SerializationError::DeserializationError(msg) => {
                write!(f, "Deserialization error: {}", msg)
            }
            SerializationError::InvalidExtensibility => write!(f, "Invalid extensibility"),
            SerializationError::InvalidMemberHeader => write!(f, "Invalid member header"),
            SerializationError::TypeHashMismatch(expected, actual) => {
                write!(f, "Type hash mismatch: expected {:?}, got {:?}", expected, actual)
            }
            SerializationError::InvalidEnumDiscriminant(d) => {
                write!(f, "Invalid enum discriminant: {}", d)
            }
            SerializationError::InvalidUnionDiscriminant(d) => {
                write!(f, "Invalid union discriminant: {}", d)
            }
            SerializationError::SliceConversionError => {
                write!(f, "Slice conversion error")
            }
            SerializationError::InvalidMemberId(id) => {
                write!(f, "EMHEADER member_id exceeds 28 bits: {:#x}", id)
            }
        }
    }
}

impl StdError for SerializationError {}
pub type SerializationResult<T> = Result<T, SerializationError>;
