//! Spec-conformant XCDR2 (little-endian) codec for `TypeObject` per DDS-XTypes 1.3.
//!
//! This is the EquivalenceHash input serialization: byte-compatible.
//! The in-memory model in [`super::type_object`] keeps its historical encodings; this
//! codec maps model<->spec on the wire (flag re-encoding, EquivalenceKind fix, union
//! field-order fix, synthesized empty headers). No encapsulation header is emitted;
//! alignment origin is the buffer start.

use super::type_object::*;

// TypeKind octets
mod tk {
    pub const NONE: u8 = 0x00;
    pub const ALIAS: u8 = 0x30;
    pub const ENUM: u8 = 0x40;
    pub const BITMASK: u8 = 0x41;
    pub const ANNOTATION: u8 = 0x50;
    pub const STRUCTURE: u8 = 0x51;
    pub const UNION: u8 = 0x52;
    pub const BITSET: u8 = 0x53;
    pub const SEQUENCE: u8 = 0x60;
    pub const ARRAY: u8 = 0x61;
    pub const MAP: u8 = 0x62;
}

const EK_MINIMAL: u8 = type_kind::EK_MINIMAL;
const EK_COMPLETE: u8 = type_kind::EK_COMPLETE;

// TypeIdentifier discriminator octets.
const TI_STRING8_SMALL: u8 = type_kind::TI_STRING8_SMALL;
const TI_STRING8_LARGE: u8 = type_kind::TI_STRING8_LARGE;
const TI_STRING16_SMALL: u8 = type_kind::TI_STRING16_SMALL;
const TI_STRING16_LARGE: u8 = type_kind::TI_STRING16_LARGE;
const TI_PLAIN_SEQUENCE_SMALL: u8 = type_kind::TI_PLAIN_SEQUENCE_SMALL;
const TI_PLAIN_SEQUENCE_LARGE: u8 = type_kind::TI_PLAIN_SEQUENCE_LARGE;
const TI_PLAIN_ARRAY_SMALL: u8 = type_kind::TI_PLAIN_ARRAY_SMALL;
const TI_PLAIN_ARRAY_LARGE: u8 = type_kind::TI_PLAIN_ARRAY_LARGE;
const TI_PLAIN_MAP_SMALL: u8 = type_kind::TI_PLAIN_MAP_SMALL;
const TI_PLAIN_MAP_LARGE: u8 = type_kind::TI_PLAIN_MAP_LARGE;

/// Serialize a `TypeObject` to the spec XCDR2 byte stream used as hash input.
pub fn serialize_type_object(obj: &TypeObject) -> Vec<u8> {
    let mut w = W::new();
    // Outer TypeObject union is APPENDABLE: DHEADER, then EquivalenceKind octet, then arm.
    let dh = w.begin_dheader();
    match obj {
        TypeObject::Minimal(m) => {
            w.u8(EK_MINIMAL);
            write_minimal_type_object(&mut w, m);
        }
        TypeObject::Complete(c) => {
            w.u8(EK_COMPLETE);
            write_complete_type_object(&mut w, c);
        }
    }
    w.end_dheader(dh);
    w.buf
}

/// Deserialize a `TypeObject` from the spec XCDR2 byte stream.
pub fn deserialize_type_object(data: &[u8]) -> Result<TypeObject, String> {
    deserialize_type_object_with_len(data).map(|(obj, _)| obj)
}

/// Deserialize a `TypeObject` and return the number of bytes consumed (DHEADER + value).
pub fn deserialize_type_object_with_len(data: &[u8]) -> Result<(TypeObject, usize), String> {
    let mut r = R::new(data);
    let end = r.begin_dheader()?;
    let ek = r.u8()?;
    let obj = match ek {
        EK_MINIMAL => TypeObject::Minimal(read_minimal_type_object(&mut r)?),
        EK_COMPLETE => TypeObject::Complete(read_complete_type_object(&mut r)?),
        _ => return Err(format!("unknown TypeObject EquivalenceKind: 0x{:02X}", ek)),
    };
    r.set_pos(end);
    Ok((obj, end))
}

/// Compute the 14-byte EquivalenceHash = MD5(spec XCDR2 bytes)[0..14].
pub fn spec_hash(obj: &TypeObject) -> EquivalenceHash {
    EquivalenceHash::compute(&serialize_type_object(obj))
}

struct W {
    buf: Vec<u8>,
}

impl W {
    fn new() -> Self {
        Self { buf: Vec::new() }
    }

    fn align(&mut self, a: usize) {
        while self.buf.len() % a != 0 {
            self.buf.push(0);
        }
    }

    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    fn u16(&mut self, v: u16) {
        self.align(2);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn u32(&mut self, v: u32) {
        self.align(4);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn i32(&mut self, v: i32) {
        self.align(4);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn u64(&mut self, v: u64) {
        self.align(4);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn i64(&mut self, v: i64) {
        self.align(4);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn f32(&mut self, v: f32) {
        self.align(4);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn f64(&mut self, v: f64) {
        self.align(4);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn bool(&mut self, v: bool) {
        self.buf.push(v as u8);
    }

    fn bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }

    fn string(&mut self, s: &str) {
        let b = s.as_bytes();
        self.u32(b.len() as u32 + 1);
        self.buf.extend_from_slice(b);
        self.buf.push(0);
    }

    /// wstring value: u32 code-unit count + UTF-16LE code units (no NUL).
    fn wstring(&mut self, s: &str) {
        let units: Vec<u16> = s.encode_utf16().collect();
        self.u32(units.len() as u32);
        for u in units {
            self.buf.extend_from_slice(&u.to_le_bytes());
        }
    }

    fn begin_dheader(&mut self) -> usize {
        self.align(4);
        let pos = self.buf.len();
        self.buf.extend_from_slice(&[0, 0, 0, 0]);
        pos
    }

    fn end_dheader(&mut self, pos: usize) {
        let len = (self.buf.len() - (pos + 4)) as u32;
        self.buf[pos..pos + 4].copy_from_slice(&len.to_le_bytes());
    }
}

struct R<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> R<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    fn align(&mut self, a: usize) {
        let m = self.pos % a;
        if m != 0 {
            self.pos += a - m;
        }
    }

    fn need(&self, n: usize) -> Result<(), String> {
        if self.pos + n > self.buf.len() {
            Err("unexpected end of TypeObject buffer".to_string())
        } else {
            Ok(())
        }
    }

    fn u8(&mut self) -> Result<u8, String> {
        self.need(1)?;
        let v = self.buf[self.pos];
        self.pos += 1;
        Ok(v)
    }

    fn u16(&mut self) -> Result<u16, String> {
        self.align(2);
        self.need(2)?;
        let v = u16::from_le_bytes([self.buf[self.pos], self.buf[self.pos + 1]]);
        self.pos += 2;
        Ok(v)
    }

    fn u32(&mut self) -> Result<u32, String> {
        self.align(4);
        self.need(4)?;
        let v = u32::from_le_bytes([
            self.buf[self.pos],
            self.buf[self.pos + 1],
            self.buf[self.pos + 2],
            self.buf[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(v)
    }

    fn i32(&mut self) -> Result<i32, String> {
        Ok(self.u32()? as i32)
    }

    fn u64(&mut self) -> Result<u64, String> {
        self.align(4);
        self.need(8)?;
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.buf[self.pos..self.pos + 8]);
        self.pos += 8;
        Ok(u64::from_le_bytes(b))
    }

    fn i64(&mut self) -> Result<i64, String> {
        Ok(self.u64()? as i64)
    }

    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_bits(self.u32()?))
    }

    fn f64(&mut self) -> Result<f64, String> {
        Ok(f64::from_bits(self.u64()?))
    }

    fn bool(&mut self) -> Result<bool, String> {
        Ok(self.u8()? != 0)
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8], String> {
        self.need(n)?;
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn name_hash(&mut self) -> Result<u32, String> {
        let b = self.bytes(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn string(&mut self) -> Result<String, String> {
        let len = self.u32()? as usize;
        let raw = self.bytes(len)?;
        if len == 0 {
            Ok(String::new())
        } else {
            Ok(String::from_utf8_lossy(&raw[..len - 1]).to_string())
        }
    }

    fn wstring(&mut self) -> Result<String, String> {
        let count = self.u32()? as usize;
        let raw = self.bytes(count.checked_mul(2).ok_or("wstring length overflow")?)?;
        let units: Vec<u16> =
            (0..count).map(|i| u16::from_le_bytes([raw[2 * i], raw[2 * i + 1]])).collect();
        Ok(String::from_utf16_lossy(&units))
    }

    /// Read a DHEADER; return the absolute end offset of the delimited content.
    fn begin_dheader(&mut self) -> Result<usize, String> {
        let len = self.u32()? as usize;
        let end = self.pos.checked_add(len).ok_or("TypeObject DHEADER overflow")?;
        if end > self.buf.len() {
            return Err("TypeObject DHEADER overruns buffer".to_string());
        }
        Ok(end)
    }

    /// Sequence element count sanity-capped against the buffer length so a corrupt
    /// wire count cannot drive an unbounded loop.
    fn count(&mut self) -> Result<usize, String> {
        let c = self.u32()? as usize;
        if c > self.buf.len() {
            return Err("TypeObject sequence count exceeds buffer".to_string());
        }
        Ok(c)
    }
}

fn spec_type_flags(f: TypeFlag) -> u16 {
    let mut v: u16 = match f.extensibility() {
        ExtensibilityKind::Final => 1,
        ExtensibilityKind::Appendable => 2,
        ExtensibilityKind::Mutable => 4,
    };
    if f.is_nested() {
        v |= 8;
    }
    if f.is_autoid_hash() {
        v |= 16;
    }
    v
}

fn model_type_flags(spec: u16) -> TypeFlag {
    let ext = match spec & 0x7 {
        1 => ExtensibilityKind::Final,
        2 => ExtensibilityKind::Appendable,
        4 => ExtensibilityKind::Mutable,
        _ => ExtensibilityKind::Final,
    };
    TypeFlag::new(ext, spec & 0x8 != 0, spec & 0x10 != 0)
}

fn spec_member_flags(m: MemberFlag) -> u16 {
    let raw = m.0;
    (raw & !0x3) | ((raw & 0x3) + 1)
}

fn model_member_flags(spec: u16) -> MemberFlag {
    let tc = (spec & 0x3).saturating_sub(1);
    MemberFlag((spec & !0x3) | tc)
}

fn spec_collection_element_flags(c: CollectionElementFlag) -> u16 {
    let raw = c.0 as u16;
    ((raw & 0x3) + 1) | (raw & 0x4)
}

fn model_collection_element_flags(spec: u16) -> CollectionElementFlag {
    let tc = (spec & 0x3).saturating_sub(1);
    CollectionElementFlag(((spec & 0x4) | tc) as u8)
}

fn spec_literal_flags(f: EnumeratedLiteralFlag) -> u16 {
    if f.is_default() {
        0x40
    } else {
        0
    }
}

fn model_literal_flags(spec: u16) -> EnumeratedLiteralFlag {
    EnumeratedLiteralFlag(if spec & 0x40 != 0 { 0x01 } else { 0 })
}

fn write_type_identifier(w: &mut W, ti: &TypeIdentifier) {
    use TypeIdentifier::*;
    match ti {
        None => w.u8(tk::NONE),
        Boolean => w.u8(type_kind::TK_BOOLEAN),
        Byte => w.u8(type_kind::TK_BYTE),
        Int16 => w.u8(type_kind::TK_INT16),
        Int32 => w.u8(type_kind::TK_INT32),
        Int64 => w.u8(type_kind::TK_INT64),
        Uint16 => w.u8(type_kind::TK_UINT16),
        Uint32 => w.u8(type_kind::TK_UINT32),
        Uint64 => w.u8(type_kind::TK_UINT64),
        Float32 => w.u8(type_kind::TK_FLOAT32),
        Float64 => w.u8(type_kind::TK_FLOAT64),
        Float128 => w.u8(type_kind::TK_FLOAT128),
        Int8 => w.u8(type_kind::TK_INT8),
        Uint8 => w.u8(type_kind::TK_UINT8),
        Char8 => w.u8(type_kind::TK_CHAR8),
        Char16 => w.u8(type_kind::TK_CHAR16),
        // Unbounded strings have no TypeIdentifier union case: emit SMALL with bound 0.
        String8 => {
            w.u8(TI_STRING8_SMALL);
            w.u8(0);
        }
        String16 => {
            w.u8(TI_STRING16_SMALL);
            w.u8(0);
        }
        String8Small { bound } => {
            w.u8(TI_STRING8_SMALL);
            w.u8(*bound);
        }
        String16Small { bound } => {
            w.u8(TI_STRING16_SMALL);
            w.u8(*bound);
        }
        String8Large { bound } => {
            w.u8(TI_STRING8_LARGE);
            w.u32(*bound);
        }
        String16Large { bound } => {
            w.u8(TI_STRING16_LARGE);
            w.u32(*bound);
        }
        PlainSequenceSmall { header, bound, element_identifier } => {
            w.u8(TI_PLAIN_SEQUENCE_SMALL);
            write_plain_collection_header(w, header);
            w.u8(*bound);
            write_type_identifier(w, element_identifier);
        }
        PlainSequenceLarge { header, bound, element_identifier } => {
            w.u8(TI_PLAIN_SEQUENCE_LARGE);
            write_plain_collection_header(w, header);
            w.u32(*bound);
            write_type_identifier(w, element_identifier);
        }
        PlainArraySmall { header, array_bound_seq, element_identifier } => {
            w.u8(TI_PLAIN_ARRAY_SMALL);
            write_plain_collection_header(w, header);
            w.u32(array_bound_seq.len() as u32);
            w.bytes(array_bound_seq);
            write_type_identifier(w, element_identifier);
        }
        PlainArrayLarge { header, array_bound_seq, element_identifier } => {
            w.u8(TI_PLAIN_ARRAY_LARGE);
            write_plain_collection_header(w, header);
            w.u32(array_bound_seq.len() as u32);
            for b in array_bound_seq {
                w.u32(*b);
            }
            write_type_identifier(w, element_identifier);
        }
        // Spec field order: header, bound, element_identifier, key_flags, key_identifier.
        PlainMapSmall { header, bound, key_flags, key_identifier, element_identifier } => {
            w.u8(TI_PLAIN_MAP_SMALL);
            write_plain_collection_header(w, header);
            w.u8(*bound);
            write_type_identifier(w, element_identifier);
            w.u16(spec_collection_element_flags(*key_flags));
            write_type_identifier(w, key_identifier);
        }
        PlainMapLarge { header, bound, key_flags, key_identifier, element_identifier } => {
            w.u8(TI_PLAIN_MAP_LARGE);
            write_plain_collection_header(w, header);
            w.u32(*bound);
            write_type_identifier(w, element_identifier);
            w.u16(spec_collection_element_flags(*key_flags));
            write_type_identifier(w, key_identifier);
        }
        MinimalTypeId(hash) => {
            w.u8(EK_MINIMAL);
            w.bytes(hash.as_bytes());
        }
        CompleteTypeId(hash) => {
            w.u8(EK_COMPLETE);
            w.bytes(hash.as_bytes());
        }
    }
}

fn write_plain_collection_header(w: &mut W, h: &PlainCollectionHeader) {
    w.u8(h.equiv_kind as u8);
    w.u16(spec_collection_element_flags(h.element_flags));
}

fn read_plain_collection_header(r: &mut R) -> Result<PlainCollectionHeader, String> {
    let equiv_kind = EquivalenceKind::from_u8(r.u8()?);
    let element_flags = model_collection_element_flags(r.u16()?);
    Ok(PlainCollectionHeader { equiv_kind, element_flags })
}

fn read_type_identifier(r: &mut R) -> Result<TypeIdentifier, String> {
    use type_kind as t;
    let k = r.u8()?;
    let ti = match k {
        tk::NONE => TypeIdentifier::None,
        t::TK_BOOLEAN => TypeIdentifier::Boolean,
        t::TK_BYTE => TypeIdentifier::Byte,
        t::TK_INT16 => TypeIdentifier::Int16,
        t::TK_INT32 => TypeIdentifier::Int32,
        t::TK_INT64 => TypeIdentifier::Int64,
        t::TK_UINT16 => TypeIdentifier::Uint16,
        t::TK_UINT32 => TypeIdentifier::Uint32,
        t::TK_UINT64 => TypeIdentifier::Uint64,
        t::TK_FLOAT32 => TypeIdentifier::Float32,
        t::TK_FLOAT64 => TypeIdentifier::Float64,
        t::TK_FLOAT128 => TypeIdentifier::Float128,
        t::TK_INT8 => TypeIdentifier::Int8,
        t::TK_UINT8 => TypeIdentifier::Uint8,
        t::TK_CHAR8 => TypeIdentifier::Char8,
        t::TK_CHAR16 => TypeIdentifier::Char16,
        TI_STRING8_SMALL => {
            let b = r.u8()?;
            if b == 0 {
                TypeIdentifier::String8
            } else {
                TypeIdentifier::String8Small { bound: b }
            }
        }
        TI_STRING16_SMALL => {
            let b = r.u8()?;
            if b == 0 {
                TypeIdentifier::String16
            } else {
                TypeIdentifier::String16Small { bound: b }
            }
        }
        TI_STRING8_LARGE => TypeIdentifier::String8Large { bound: r.u32()? },
        TI_STRING16_LARGE => TypeIdentifier::String16Large { bound: r.u32()? },
        TI_PLAIN_SEQUENCE_SMALL => {
            let header = read_plain_collection_header(r)?;
            let bound = r.u8()?;
            let element = read_type_identifier(r)?;
            TypeIdentifier::PlainSequenceSmall {
                header,
                bound,
                element_identifier: Box::new(element),
            }
        }
        TI_PLAIN_SEQUENCE_LARGE => {
            let header = read_plain_collection_header(r)?;
            let bound = r.u32()?;
            let element = read_type_identifier(r)?;
            TypeIdentifier::PlainSequenceLarge {
                header,
                bound,
                element_identifier: Box::new(element),
            }
        }
        TI_PLAIN_ARRAY_SMALL => {
            let header = read_plain_collection_header(r)?;
            let n = r.count()?;
            let array_bound_seq = r.bytes(n)?.to_vec();
            let element = read_type_identifier(r)?;
            TypeIdentifier::PlainArraySmall {
                header,
                array_bound_seq,
                element_identifier: Box::new(element),
            }
        }
        TI_PLAIN_ARRAY_LARGE => {
            let header = read_plain_collection_header(r)?;
            let n = r.count()?;
            let mut array_bound_seq = Vec::new();
            for _ in 0..n {
                array_bound_seq.push(r.u32()?);
            }
            let element = read_type_identifier(r)?;
            TypeIdentifier::PlainArrayLarge {
                header,
                array_bound_seq,
                element_identifier: Box::new(element),
            }
        }
        TI_PLAIN_MAP_SMALL => {
            let header = read_plain_collection_header(r)?;
            let bound = r.u8()?;
            let element = read_type_identifier(r)?;
            let key_flags = model_collection_element_flags(r.u16()?);
            let key = read_type_identifier(r)?;
            TypeIdentifier::PlainMapSmall {
                header,
                bound,
                key_flags,
                key_identifier: Box::new(key),
                element_identifier: Box::new(element),
            }
        }
        TI_PLAIN_MAP_LARGE => {
            let header = read_plain_collection_header(r)?;
            let bound = r.u32()?;
            let element = read_type_identifier(r)?;
            let key_flags = model_collection_element_flags(r.u16()?);
            let key = read_type_identifier(r)?;
            TypeIdentifier::PlainMapLarge {
                header,
                bound,
                key_flags,
                key_identifier: Box::new(key),
                element_identifier: Box::new(element),
            }
        }
        EK_MINIMAL => TypeIdentifier::MinimalTypeId(read_hash(r)?),
        EK_COMPLETE => TypeIdentifier::CompleteTypeId(read_hash(r)?),
        _ => return Err(format!("unsupported TypeIdentifier kind: 0x{:02X}", k)),
    };
    Ok(ti)
}

fn read_hash(r: &mut R) -> Result<EquivalenceHash, String> {
    let b = r.bytes(14)?;
    let mut h = [0u8; 14];
    h.copy_from_slice(b);
    Ok(EquivalenceHash::new(h))
}

fn primitive_from_kind(k: u8) -> TypeIdentifier {
    use type_kind as t;
    match k {
        t::TK_BOOLEAN => TypeIdentifier::Boolean,
        t::TK_BYTE => TypeIdentifier::Byte,
        t::TK_INT16 => TypeIdentifier::Int16,
        t::TK_INT32 => TypeIdentifier::Int32,
        t::TK_INT64 => TypeIdentifier::Int64,
        t::TK_UINT16 => TypeIdentifier::Uint16,
        t::TK_UINT32 => TypeIdentifier::Uint32,
        t::TK_UINT64 => TypeIdentifier::Uint64,
        t::TK_INT8 => TypeIdentifier::Int8,
        t::TK_UINT8 => TypeIdentifier::Uint8,
        t::TK_CHAR8 => TypeIdentifier::Char8,
        t::TK_CHAR16 => TypeIdentifier::Char16,
        _ => TypeIdentifier::None,
    }
}

fn write_annotation_parameter_value(w: &mut W, v: &AnnotationParameterValue) {
    use AnnotationParameterValue as A;
    match v {
        A::BooleanValue(b) => {
            w.u8(type_kind::TK_BOOLEAN);
            w.bool(*b);
        }
        A::ByteValue(x) => {
            w.u8(type_kind::TK_BYTE);
            w.u8(*x);
        }
        A::Int16Value(x) => {
            w.u8(type_kind::TK_INT16);
            w.u16(*x as u16);
        }
        A::Uint16Value(x) => {
            w.u8(type_kind::TK_UINT16);
            w.u16(*x);
        }
        A::Int32Value(x) => {
            w.u8(type_kind::TK_INT32);
            w.i32(*x);
        }
        A::Uint32Value(x) => {
            w.u8(type_kind::TK_UINT32);
            w.u32(*x);
        }
        A::Int64Value(x) => {
            w.u8(type_kind::TK_INT64);
            w.i64(*x);
        }
        A::Uint64Value(x) => {
            w.u8(type_kind::TK_UINT64);
            w.u64(*x);
        }
        A::Float32Value(x) => {
            w.u8(type_kind::TK_FLOAT32);
            w.f32(*x);
        }
        A::Float64Value(x) => {
            w.u8(type_kind::TK_FLOAT64);
            w.f64(*x);
        }
        A::CharValue(x) => {
            w.u8(type_kind::TK_CHAR8);
            w.u8(*x);
        }
        A::WcharValue(x) => {
            w.u8(type_kind::TK_CHAR16);
            w.u16(*x);
        }
        A::StringValue(s) => {
            w.u8(type_kind::TK_STRING8);
            w.string(s);
        }
        A::WstringValue(s) => {
            w.u8(type_kind::TK_STRING16);
            w.wstring(s);
        }
        A::EnumValue(x) => {
            w.u8(tk::ENUM);
            w.i32(*x);
        }
    }
}

fn read_annotation_parameter_value(r: &mut R) -> Result<AnnotationParameterValue, String> {
    use type_kind as t;
    use AnnotationParameterValue as A;
    let k = r.u8()?;
    let v = match k {
        t::TK_BOOLEAN => A::BooleanValue(r.bool()?),
        t::TK_BYTE => A::ByteValue(r.u8()?),
        t::TK_INT16 => A::Int16Value(r.u16()? as i16),
        t::TK_UINT16 => A::Uint16Value(r.u16()?),
        t::TK_INT32 => A::Int32Value(r.i32()?),
        t::TK_UINT32 => A::Uint32Value(r.u32()?),
        t::TK_INT64 => A::Int64Value(r.i64()?),
        t::TK_UINT64 => A::Uint64Value(r.u64()?),
        t::TK_FLOAT32 => A::Float32Value(r.f32()?),
        t::TK_FLOAT64 => A::Float64Value(r.f64()?),
        t::TK_CHAR8 => A::CharValue(r.u8()?),
        t::TK_CHAR16 => A::WcharValue(r.u16()?),
        t::TK_STRING8 => A::StringValue(r.string()?),
        t::TK_STRING16 => A::WstringValue(r.wstring()?),
        tk::ENUM => A::EnumValue(r.i32()?),
        _ => return Err(format!("unsupported AnnotationParameterValue kind: 0x{:02X}", k)),
    };
    Ok(v)
}

/// AppliedAnnotationSeq (@optional): present iff non-empty.
fn write_opt_ann_custom(w: &mut W, ann: &[AppliedAnnotation]) {
    if ann.is_empty() {
        w.bool(false);
        return;
    }
    w.bool(true);
    let dh = w.begin_dheader();
    w.u32(ann.len() as u32);
    for a in ann {
        write_applied_annotation(w, a);
    }
    w.end_dheader(dh);
}

fn read_opt_ann_custom(r: &mut R) -> Result<Vec<AppliedAnnotation>, String> {
    if !r.bool()? {
        return Ok(Vec::new());
    }
    let end = r.begin_dheader()?;
    let n = r.count()?;
    let mut out = Vec::new();
    for _ in 0..n {
        out.push(read_applied_annotation(r)?);
    }
    r.set_pos(end);
    Ok(out)
}

fn write_applied_annotation(w: &mut W, a: &AppliedAnnotation) {
    let dh = w.begin_dheader();
    write_type_identifier(w, &a.annotation_typeid);
    if a.param_seq.is_empty() {
        w.bool(false);
    } else {
        w.bool(true);
        let pdh = w.begin_dheader();
        w.u32(a.param_seq.len() as u32);
        for (name_hash, value) in &a.param_seq {
            let edh = w.begin_dheader();
            w.bytes(&name_hash.to_be_bytes());
            write_annotation_parameter_value(w, value);
            w.end_dheader(edh);
        }
        w.end_dheader(pdh);
    }
    w.end_dheader(dh);
}

fn read_applied_annotation(r: &mut R) -> Result<AppliedAnnotation, String> {
    let end = r.begin_dheader()?;
    let annotation_typeid = read_type_identifier(r)?;
    let mut param_seq = Vec::new();
    if r.bool()? {
        let pend = r.begin_dheader()?;
        let n = r.count()?;
        for _ in 0..n {
            let eend = r.begin_dheader()?;
            let name_hash = r.name_hash()?;
            let value = read_annotation_parameter_value(r)?;
            r.set_pos(eend);
            param_seq.push((name_hash, value));
        }
        r.set_pos(pend);
    }
    r.set_pos(end);
    Ok(AppliedAnnotation { annotation_typeid, param_seq })
}

/// AppliedBuiltinMemberAnnotations (@optional, APPENDABLE): present iff Some && !is_empty.
fn write_opt_member_ann_builtin(w: &mut W, ann: &Option<AppliedBuiltinMemberAnnotations>) {
    match ann {
        Some(a) if !a.is_empty() => {
            w.bool(true);
            let dh = w.begin_dheader();
            write_opt_string(w, &a.unit);
            write_opt_apv(w, &a.min);
            write_opt_apv(w, &a.max);
            write_opt_string(w, &a.hash_id);
            w.end_dheader(dh);
        }
        _ => w.bool(false),
    }
}

fn read_opt_member_ann_builtin(
    r: &mut R,
) -> Result<Option<AppliedBuiltinMemberAnnotations>, String> {
    if !r.bool()? {
        return Ok(None);
    }
    let end = r.begin_dheader()?;
    let unit = read_opt_string(r)?;
    let min = read_opt_apv(r)?;
    let max = read_opt_apv(r)?;
    let hash_id = read_opt_string(r)?;
    r.set_pos(end);
    Ok(Some(AppliedBuiltinMemberAnnotations { unit, min, max, hash_id }))
}

fn write_opt_string(w: &mut W, s: &Option<String>) {
    match s {
        Some(v) => {
            w.bool(true);
            w.string(v);
        }
        None => w.bool(false),
    }
}

fn read_opt_string(r: &mut R) -> Result<Option<String>, String> {
    if r.bool()? {
        Ok(Some(r.string()?))
    } else {
        Ok(None)
    }
}

fn write_opt_apv(w: &mut W, v: &Option<AnnotationParameterValue>) {
    match v {
        Some(val) => {
            w.bool(true);
            write_annotation_parameter_value(w, val);
        }
        None => w.bool(false),
    }
}

fn read_opt_apv(r: &mut R) -> Result<Option<AnnotationParameterValue>, String> {
    if r.bool()? {
        Ok(Some(read_annotation_parameter_value(r)?))
    } else {
        Ok(None)
    }
}

fn write_complete_member_detail(w: &mut W, d: &CompleteMemberDetail) {
    w.string(&d.name);
    write_opt_member_ann_builtin(w, &d.ann_builtin);
    write_opt_ann_custom(w, &d.ann_custom);
}

fn read_complete_member_detail(r: &mut R) -> Result<CompleteMemberDetail, String> {
    let name = r.string()?;
    let ann_builtin = read_opt_member_ann_builtin(r)?;
    let ann_custom = read_opt_ann_custom(r)?;
    Ok(CompleteMemberDetail { name, ann_builtin, ann_custom })
}

/// CompleteTypeDetail spec field order: @optional ann_builtin, @optional ann_custom, type_name.
/// The model's builtin type annotations have no spec slot -> always emit absent.
fn write_complete_type_detail(w: &mut W, d: &CompleteTypeDetail) {
    w.bool(false);
    write_opt_ann_custom(w, &d.ann_custom);
    w.string(&d.type_name);
}

fn read_complete_type_detail(r: &mut R) -> Result<CompleteTypeDetail, String> {
    if r.bool()? {
        // Present builtin type annotations: skip via their DHEADER, store None.
        let end = r.begin_dheader()?;
        r.set_pos(end);
    }
    let ann_custom = read_opt_ann_custom(r)?;
    let type_name = r.string()?;
    Ok(CompleteTypeDetail { type_name, ann_builtin: None, ann_custom })
}

fn write_common_struct_member(w: &mut W, c: &CommonStructMember) {
    w.u32(c.member_id);
    w.u16(spec_member_flags(c.member_flags));
    write_type_identifier(w, &c.member_type_id);
}

fn read_common_struct_member(r: &mut R) -> Result<CommonStructMember, String> {
    let member_id = r.u32()?;
    let member_flags = model_member_flags(r.u16()?);
    let member_type_id = read_type_identifier(r)?;
    Ok(CommonStructMember { member_id, member_flags, member_type_id })
}

fn write_base_type(w: &mut W, base: &Option<TypeIdentifier>) {
    match base {
        Some(ti) => write_type_identifier(w, ti),
        Option::None => w.u8(tk::NONE),
    }
}

fn read_base_type(r: &mut R) -> Result<Option<TypeIdentifier>, String> {
    let ti = read_type_identifier(r)?;
    Ok(if ti == TypeIdentifier::None { None } else { Some(ti) })
}

fn write_minimal_struct(w: &mut W, s: &MinimalStructType) {
    w.u16(spec_type_flags(s.struct_flags));
    // MinimalStructHeader (APPENDABLE): base_type + MinimalTypeDetail (empty).
    let dh = w.begin_dheader();
    write_base_type(w, &s.header.base_type);
    w.end_dheader(dh);
    let sdh = w.begin_dheader();
    w.u32(s.member_seq.len() as u32);
    for m in &s.member_seq {
        let mdh = w.begin_dheader();
        write_common_struct_member(w, &m.common);
        w.bytes(&m.name_hash.to_be_bytes());
        w.end_dheader(mdh);
    }
    w.end_dheader(sdh);
}

fn read_minimal_struct(r: &mut R) -> Result<MinimalStructType, String> {
    let struct_flags = model_type_flags(r.u16()?);
    let hend = r.begin_dheader()?;
    let base_type = read_base_type(r)?;
    r.set_pos(hend);
    let send = r.begin_dheader()?;
    let n = r.count()?;
    let mut member_seq = Vec::new();
    for _ in 0..n {
        let mend = r.begin_dheader()?;
        let common = read_common_struct_member(r)?;
        let name_hash = r.name_hash()?;
        r.set_pos(mend);
        member_seq.push(MinimalStructMember { common, name_hash });
    }
    r.set_pos(send);
    Ok(MinimalStructType { struct_flags, header: MinimalStructHeader { base_type }, member_seq })
}

fn write_complete_struct(w: &mut W, s: &CompleteStructType) {
    w.u16(spec_type_flags(s.struct_flags));
    // CompleteStructHeader (APPENDABLE): base_type + CompleteTypeDetail.
    let dh = w.begin_dheader();
    write_base_type(w, &s.header.base_type);
    write_complete_type_detail(w, &s.header.detail);
    w.end_dheader(dh);
    let sdh = w.begin_dheader();
    w.u32(s.member_seq.len() as u32);
    for m in &s.member_seq {
        let mdh = w.begin_dheader();
        write_common_struct_member(w, &m.common);
        write_complete_member_detail(w, &m.detail);
        w.end_dheader(mdh);
    }
    w.end_dheader(sdh);
}

fn read_complete_struct(r: &mut R) -> Result<CompleteStructType, String> {
    let struct_flags = model_type_flags(r.u16()?);
    let hend = r.begin_dheader()?;
    let base_type = read_base_type(r)?;
    let detail = read_complete_type_detail(r)?;
    r.set_pos(hend);
    let send = r.begin_dheader()?;
    let n = r.count()?;
    let mut member_seq = Vec::new();
    for _ in 0..n {
        let mend = r.begin_dheader()?;
        let common = read_common_struct_member(r)?;
        let detail = read_complete_member_detail(r)?;
        r.set_pos(mend);
        member_seq.push(CompleteStructMember { common, detail });
    }
    r.set_pos(send);
    Ok(CompleteStructType {
        struct_flags,
        header: CompleteStructHeader { base_type, detail },
        member_seq,
    })
}

fn write_common_union_member(w: &mut W, c: &CommonUnionMember) {
    w.u32(c.member_id);
    w.u16(spec_member_flags(c.member_flags));
    write_type_identifier(w, &c.member_type_id);
    // UnionCaseLabelSeq: primitive sequence -> count + i32 elements, no DHEADER.
    w.u32(c.label_seq.len() as u32);
    for l in &c.label_seq {
        w.i32(*l);
    }
}

fn read_common_union_member(r: &mut R) -> Result<CommonUnionMember, String> {
    let member_id = r.u32()?;
    let member_flags = model_member_flags(r.u16()?);
    let member_type_id = read_type_identifier(r)?;
    let n = r.count()?;
    let mut label_seq = Vec::new();
    for _ in 0..n {
        label_seq.push(r.i32()?);
    }
    Ok(CommonUnionMember { member_id, member_flags, member_type_id, label_seq })
}

fn write_common_discriminator(w: &mut W, c: &CommonDiscriminatorMember) {
    w.u16(spec_member_flags(c.member_flags));
    write_type_identifier(w, &c.type_id);
}

fn read_common_discriminator(r: &mut R) -> Result<CommonDiscriminatorMember, String> {
    let member_flags = model_member_flags(r.u16()?);
    let type_id = read_type_identifier(r)?;
    Ok(CommonDiscriminatorMember { member_flags, type_id })
}

fn write_minimal_union(w: &mut W, u: &MinimalUnionType) {
    w.u16(spec_type_flags(u.union_flags));
    // MinimalUnionHeader (APPENDABLE): empty MinimalTypeDetail.
    let hdh = w.begin_dheader();
    w.end_dheader(hdh);
    // MinimalDiscriminatorMember (APPENDABLE): common.
    let ddh = w.begin_dheader();
    write_common_discriminator(w, &u.discriminator);
    w.end_dheader(ddh);
    let sdh = w.begin_dheader();
    w.u32(u.member_seq.len() as u32);
    for m in &u.member_seq {
        let mdh = w.begin_dheader();
        write_common_union_member(w, &m.common);
        w.bytes(&m.name_hash.to_be_bytes());
        w.end_dheader(mdh);
    }
    w.end_dheader(sdh);
}

fn read_minimal_union(r: &mut R) -> Result<MinimalUnionType, String> {
    let union_flags = model_type_flags(r.u16()?);
    let hend = r.begin_dheader()?;
    r.set_pos(hend);
    let dend = r.begin_dheader()?;
    let discriminator = read_common_discriminator(r)?;
    r.set_pos(dend);
    let send = r.begin_dheader()?;
    let n = r.count()?;
    let mut member_seq = Vec::new();
    for _ in 0..n {
        let mend = r.begin_dheader()?;
        let common = read_common_union_member(r)?;
        let name_hash = r.name_hash()?;
        r.set_pos(mend);
        member_seq.push(MinimalUnionMember { common, name_hash });
    }
    r.set_pos(send);
    Ok(MinimalUnionType { union_flags, discriminator, member_seq })
}

fn write_complete_union(w: &mut W, u: &CompleteUnionType) {
    w.u16(spec_type_flags(u.union_flags));
    // CompleteUnionHeader (APPENDABLE): CompleteTypeDetail.
    let hdh = w.begin_dheader();
    write_complete_type_detail(w, &u.header);
    w.end_dheader(hdh);
    // CompleteDiscriminatorMember (APPENDABLE): common + 2 absent optionals.
    let ddh = w.begin_dheader();
    write_common_discriminator(w, &u.discriminator);
    w.bool(false);
    w.bool(false);
    w.end_dheader(ddh);
    let sdh = w.begin_dheader();
    w.u32(u.member_seq.len() as u32);
    for m in &u.member_seq {
        let mdh = w.begin_dheader();
        write_common_union_member(w, &m.common);
        write_complete_member_detail(w, &m.detail);
        w.end_dheader(mdh);
    }
    w.end_dheader(sdh);
}

fn read_complete_union(r: &mut R) -> Result<CompleteUnionType, String> {
    let union_flags = model_type_flags(r.u16()?);
    let hend = r.begin_dheader()?;
    let header = read_complete_type_detail(r)?;
    r.set_pos(hend);
    let dend = r.begin_dheader()?;
    let discriminator = read_common_discriminator(r)?;
    r.set_pos(dend);
    let send = r.begin_dheader()?;
    let n = r.count()?;
    let mut member_seq = Vec::new();
    for _ in 0..n {
        let mend = r.begin_dheader()?;
        let common = read_common_union_member(r)?;
        let detail = read_complete_member_detail(r)?;
        r.set_pos(mend);
        member_seq.push(CompleteUnionMember { common, detail });
    }
    r.set_pos(send);
    Ok(CompleteUnionType { union_flags, discriminator, header, member_seq })
}

fn write_common_enum_literal(w: &mut W, value: i32, flags: EnumeratedLiteralFlag) {
    // CommonEnumeratedLiteral is APPENDABLE.
    let cdh = w.begin_dheader();
    w.i32(value);
    w.u16(spec_literal_flags(flags));
    w.end_dheader(cdh);
}

fn read_common_enum_literal(r: &mut R) -> Result<CommonEnumeratedLiteral, String> {
    let cend = r.begin_dheader()?;
    let value = r.i32()?;
    let flags = model_literal_flags(r.u16()?);
    r.set_pos(cend);
    Ok(CommonEnumeratedLiteral { value, flags })
}

fn write_minimal_enum(w: &mut W, e: &MinimalEnumeratedType) {
    w.u16(0); // enum_flags unused
              // MinimalEnumeratedHeader (APPENDABLE): CommonEnumeratedHeader (bit_bound).
    let hdh = w.begin_dheader();
    w.u16(e.header.common.bit_bound);
    w.end_dheader(hdh);
    let sdh = w.begin_dheader();
    w.u32(e.literal_seq.len() as u32);
    for lit in &e.literal_seq {
        let ldh = w.begin_dheader();
        write_common_enum_literal(w, lit.common.value, lit.common.flags);
        w.bytes(&lit.name_hash.to_be_bytes());
        w.end_dheader(ldh);
    }
    w.end_dheader(sdh);
}

fn read_minimal_enum(r: &mut R) -> Result<MinimalEnumeratedType, String> {
    let _ = r.u16()?;
    let hend = r.begin_dheader()?;
    let bit_bound = r.u16()?;
    r.set_pos(hend);
    let send = r.begin_dheader()?;
    let n = r.count()?;
    let mut literal_seq = Vec::new();
    for _ in 0..n {
        let lend = r.begin_dheader()?;
        let common = read_common_enum_literal(r)?;
        let name_hash = r.name_hash()?;
        r.set_pos(lend);
        literal_seq.push(MinimalEnumeratedLiteral { common, name_hash });
    }
    r.set_pos(send);
    Ok(MinimalEnumeratedType {
        enum_flags: TypeFlag::default(),
        header: MinimalEnumeratedHeader { common: CommonEnumeratedHeader { bit_bound } },
        literal_seq,
    })
}

fn write_complete_enum(w: &mut W, e: &CompleteEnumeratedType) {
    w.u16(0);
    // CompleteEnumeratedHeader (APPENDABLE): CommonEnumeratedHeader + CompleteTypeDetail.
    let hdh = w.begin_dheader();
    w.u16(e.header.common.bit_bound);
    write_complete_type_detail(w, &e.header.detail);
    w.end_dheader(hdh);
    let sdh = w.begin_dheader();
    w.u32(e.literal_seq.len() as u32);
    for lit in &e.literal_seq {
        let ldh = w.begin_dheader();
        write_common_enum_literal(w, lit.common.value, lit.common.flags);
        write_complete_member_detail(w, &lit.detail);
        w.end_dheader(ldh);
    }
    w.end_dheader(sdh);
}

fn read_complete_enum(r: &mut R) -> Result<CompleteEnumeratedType, String> {
    let _ = r.u16()?;
    let hend = r.begin_dheader()?;
    let bit_bound = r.u16()?;
    let detail = read_complete_type_detail(r)?;
    r.set_pos(hend);
    let send = r.begin_dheader()?;
    let n = r.count()?;
    let mut literal_seq = Vec::new();
    for _ in 0..n {
        let lend = r.begin_dheader()?;
        let common = read_common_enum_literal(r)?;
        let detail = read_complete_member_detail(r)?;
        r.set_pos(lend);
        literal_seq.push(CompleteEnumeratedLiteral { common, detail });
    }
    r.set_pos(send);
    Ok(CompleteEnumeratedType {
        enum_flags: TypeFlag::default(),
        header: CompleteEnumeratedHeader { common: CommonEnumeratedHeader { bit_bound }, detail },
        literal_seq,
    })
}

fn write_common_alias_body(w: &mut W, b: &CommonAliasBody) {
    w.u16(spec_member_flags(b.related_flags));
    write_type_identifier(w, &b.related_type);
}

fn read_common_alias_body(r: &mut R) -> Result<CommonAliasBody, String> {
    let related_flags = model_member_flags(r.u16()?);
    let related_type = read_type_identifier(r)?;
    Ok(CommonAliasBody { related_flags, related_type })
}

fn write_minimal_alias(w: &mut W, a: &MinimalAliasType) {
    w.u16(0); // alias_flags unused
              // MinimalAliasHeader (APPENDABLE): empty.
    let hdh = w.begin_dheader();
    w.end_dheader(hdh);
    // MinimalAliasBody (APPENDABLE): CommonAliasBody.
    let bdh = w.begin_dheader();
    write_common_alias_body(w, &a.body);
    w.end_dheader(bdh);
}

fn read_minimal_alias(r: &mut R) -> Result<MinimalAliasType, String> {
    let _ = r.u16()?;
    let hend = r.begin_dheader()?;
    r.set_pos(hend);
    let bend = r.begin_dheader()?;
    let body = read_common_alias_body(r)?;
    r.set_pos(bend);
    Ok(MinimalAliasType { alias_flags: TypeFlag::default(), body })
}

fn write_complete_alias(w: &mut W, a: &CompleteAliasType) {
    w.u16(0);
    // CompleteAliasHeader (APPENDABLE): CompleteTypeDetail.
    let hdh = w.begin_dheader();
    write_complete_type_detail(w, &a.header);
    w.end_dheader(hdh);
    // CompleteAliasBody (APPENDABLE): CommonAliasBody + 2 absent optionals.
    let bdh = w.begin_dheader();
    write_common_alias_body(w, &a.body);
    w.bool(false);
    w.bool(false);
    w.end_dheader(bdh);
}

fn read_complete_alias(r: &mut R) -> Result<CompleteAliasType, String> {
    let _ = r.u16()?;
    let hend = r.begin_dheader()?;
    let header = read_complete_type_detail(r)?;
    r.set_pos(hend);
    let bend = r.begin_dheader()?;
    let body = read_common_alias_body(r)?;
    r.set_pos(bend);
    Ok(CompleteAliasType { alias_flags: TypeFlag::default(), header, body })
}

fn write_minimal_bitmask(w: &mut W, b: &MinimalBitmaskType) {
    let bdh = w.begin_dheader();
    w.u16(0); // bitmask_flags unused
              // MinimalEnumeratedHeader (APPENDABLE): CommonEnumeratedHeader.
    let hdh = w.begin_dheader();
    w.u16(b.header.bit_bound);
    w.end_dheader(hdh);
    let sdh = w.begin_dheader();
    w.u32(b.flag_seq.len() as u32);
    for f in &b.flag_seq {
        let fdh = w.begin_dheader();
        w.u16(f.common.position);
        w.u16(0); // BitflagFlag unused
        w.bytes(&f.name_hash.to_be_bytes());
        w.end_dheader(fdh);
    }
    w.end_dheader(sdh);
    w.end_dheader(bdh);
}

fn read_minimal_bitmask(r: &mut R) -> Result<MinimalBitmaskType, String> {
    let bend = r.begin_dheader()?;
    let _ = r.u16()?;
    let hend = r.begin_dheader()?;
    let bit_bound = r.u16()?;
    r.set_pos(hend);
    let send = r.begin_dheader()?;
    let n = r.count()?;
    let mut flag_seq = Vec::new();
    for _ in 0..n {
        let fend = r.begin_dheader()?;
        let position = r.u16()?;
        let _ = r.u16()?;
        let name_hash = r.name_hash()?;
        r.set_pos(fend);
        flag_seq.push(MinimalBitflag {
            common: CommonBitflag { position, flags: MemberFlag::default() },
            name_hash,
        });
    }
    r.set_pos(send);
    r.set_pos(bend);
    Ok(MinimalBitmaskType {
        bitmask_flags: TypeFlag::default(),
        header: CommonEnumeratedHeader { bit_bound },
        flag_seq,
    })
}

fn write_complete_bitmask(w: &mut W, b: &CompleteBitmaskType) {
    let bdh = w.begin_dheader();
    w.u16(0);
    // CompleteEnumeratedHeader (APPENDABLE): CommonEnumeratedHeader + CompleteTypeDetail.
    let hdh = w.begin_dheader();
    w.u16(b.header.common.bit_bound);
    write_complete_type_detail(w, &b.header.detail);
    w.end_dheader(hdh);
    let sdh = w.begin_dheader();
    w.u32(b.flag_seq.len() as u32);
    for f in &b.flag_seq {
        let fdh = w.begin_dheader();
        w.u16(f.common.position);
        w.u16(0);
        write_complete_member_detail(w, &f.detail);
        w.end_dheader(fdh);
    }
    w.end_dheader(sdh);
    w.end_dheader(bdh);
}

fn read_complete_bitmask(r: &mut R) -> Result<CompleteBitmaskType, String> {
    let bend = r.begin_dheader()?;
    let _ = r.u16()?;
    let hend = r.begin_dheader()?;
    let bit_bound = r.u16()?;
    let detail = read_complete_type_detail(r)?;
    r.set_pos(hend);
    let send = r.begin_dheader()?;
    let n = r.count()?;
    let mut flag_seq = Vec::new();
    for _ in 0..n {
        let fend = r.begin_dheader()?;
        let position = r.u16()?;
        let _ = r.u16()?;
        let detail = read_complete_member_detail(r)?;
        r.set_pos(fend);
        flag_seq.push(CompleteBitflag {
            common: CommonBitflag { position, flags: MemberFlag::default() },
            detail,
        });
    }
    r.set_pos(send);
    r.set_pos(bend);
    Ok(CompleteBitmaskType {
        bitmask_flags: TypeFlag::default(),
        header: CompleteEnumeratedHeader { common: CommonEnumeratedHeader { bit_bound }, detail },
        flag_seq,
    })
}

fn write_common_bitfield(w: &mut W, c: &CommonBitfield) {
    w.u16(c.position);
    w.u16(0); // BitsetMemberFlag unused
    w.u8(c.bitcount);
    w.u8(c.holder_type.discriminator()); // holder_type is a TypeKind octet
}

fn read_common_bitfield(r: &mut R) -> Result<CommonBitfield, String> {
    let position = r.u16()?;
    let _ = r.u16()?;
    let bitcount = r.u8()?;
    let holder_type = primitive_from_kind(r.u8()?);
    Ok(CommonBitfield { position, flags: MemberFlag::default(), bitcount, holder_type })
}

fn write_minimal_bitset(w: &mut W, b: &MinimalBitsetType) {
    let bdh = w.begin_dheader();
    w.u16(0); // bitset_flags unused
              // MinimalBitsetHeader (APPENDABLE): empty.
    let hdh = w.begin_dheader();
    w.end_dheader(hdh);
    let sdh = w.begin_dheader();
    w.u32(b.field_seq.len() as u32);
    for f in &b.field_seq {
        let fdh = w.begin_dheader();
        write_common_bitfield(w, &f.common);
        w.bytes(&f.name_hash.to_be_bytes());
        w.end_dheader(fdh);
    }
    w.end_dheader(sdh);
    w.end_dheader(bdh);
}

fn read_minimal_bitset(r: &mut R) -> Result<MinimalBitsetType, String> {
    let bend = r.begin_dheader()?;
    let _ = r.u16()?;
    let hend = r.begin_dheader()?;
    r.set_pos(hend);
    let send = r.begin_dheader()?;
    let n = r.count()?;
    let mut field_seq = Vec::new();
    for _ in 0..n {
        let fend = r.begin_dheader()?;
        let common = read_common_bitfield(r)?;
        let name_hash = r.name_hash()?;
        r.set_pos(fend);
        field_seq.push(MinimalBitfield { common, name_hash });
    }
    r.set_pos(send);
    r.set_pos(bend);
    Ok(MinimalBitsetType { bitset_flags: TypeFlag::default(), field_seq })
}

fn write_complete_bitset(w: &mut W, b: &CompleteBitsetType) {
    let bdh = w.begin_dheader();
    w.u16(0);
    // CompleteBitsetHeader (APPENDABLE): CompleteTypeDetail.
    let hdh = w.begin_dheader();
    write_complete_type_detail(w, &b.header);
    w.end_dheader(hdh);
    let sdh = w.begin_dheader();
    w.u32(b.field_seq.len() as u32);
    for f in &b.field_seq {
        let fdh = w.begin_dheader();
        write_common_bitfield(w, &f.common);
        write_complete_member_detail(w, &f.detail);
        w.end_dheader(fdh);
    }
    w.end_dheader(sdh);
    w.end_dheader(bdh);
}

fn read_complete_bitset(r: &mut R) -> Result<CompleteBitsetType, String> {
    let bend = r.begin_dheader()?;
    let _ = r.u16()?;
    let hend = r.begin_dheader()?;
    let header = read_complete_type_detail(r)?;
    r.set_pos(hend);
    let send = r.begin_dheader()?;
    let n = r.count()?;
    let mut field_seq = Vec::new();
    for _ in 0..n {
        let fend = r.begin_dheader()?;
        let common = read_common_bitfield(r)?;
        let detail = read_complete_member_detail(r)?;
        r.set_pos(fend);
        field_seq.push(CompleteBitfield { common, detail });
    }
    r.set_pos(send);
    r.set_pos(bend);
    Ok(CompleteBitsetType { bitset_flags: TypeFlag::default(), header, field_seq })
}

fn write_minimal_type_object(w: &mut W, m: &MinimalTypeObject) {
    match m {
        MinimalTypeObject::Struct(s) => {
            w.u8(tk::STRUCTURE);
            write_minimal_struct(w, s);
        }
        MinimalTypeObject::Union(u) => {
            w.u8(tk::UNION);
            write_minimal_union(w, u);
        }
        MinimalTypeObject::Enum(e) => {
            w.u8(tk::ENUM);
            write_minimal_enum(w, e);
        }
        MinimalTypeObject::Alias(a) => {
            w.u8(tk::ALIAS);
            write_minimal_alias(w, a);
        }
        MinimalTypeObject::Bitmask(b) => {
            w.u8(tk::BITMASK);
            write_minimal_bitmask(w, b);
        }
        MinimalTypeObject::Bitset(b) => {
            w.u8(tk::BITSET);
            write_minimal_bitset(w, b);
        }
    }
}

fn read_minimal_type_object(r: &mut R) -> Result<MinimalTypeObject, String> {
    let k = r.u8()?;
    let obj = match k {
        tk::STRUCTURE => MinimalTypeObject::Struct(read_minimal_struct(r)?),
        tk::UNION => MinimalTypeObject::Union(read_minimal_union(r)?),
        tk::ENUM => MinimalTypeObject::Enum(read_minimal_enum(r)?),
        tk::ALIAS => MinimalTypeObject::Alias(read_minimal_alias(r)?),
        tk::BITMASK => MinimalTypeObject::Bitmask(read_minimal_bitmask(r)?),
        tk::BITSET => MinimalTypeObject::Bitset(read_minimal_bitset(r)?),
        tk::ANNOTATION | tk::SEQUENCE | tk::ARRAY | tk::MAP => {
            return Err("unsupported TypeObject kind".to_string())
        }
        _ => return Err("unsupported TypeObject kind".to_string()),
    };
    Ok(obj)
}

fn write_complete_type_object(w: &mut W, c: &CompleteTypeObject) {
    match c {
        CompleteTypeObject::Struct(s) => {
            w.u8(tk::STRUCTURE);
            write_complete_struct(w, s);
        }
        CompleteTypeObject::Union(u) => {
            w.u8(tk::UNION);
            write_complete_union(w, u);
        }
        CompleteTypeObject::Enum(e) => {
            w.u8(tk::ENUM);
            write_complete_enum(w, e);
        }
        CompleteTypeObject::Alias(a) => {
            w.u8(tk::ALIAS);
            write_complete_alias(w, a);
        }
        CompleteTypeObject::Bitmask(b) => {
            w.u8(tk::BITMASK);
            write_complete_bitmask(w, b);
        }
        CompleteTypeObject::Bitset(b) => {
            w.u8(tk::BITSET);
            write_complete_bitset(w, b);
        }
    }
}

fn read_complete_type_object(r: &mut R) -> Result<CompleteTypeObject, String> {
    let k = r.u8()?;
    let obj = match k {
        tk::STRUCTURE => CompleteTypeObject::Struct(read_complete_struct(r)?),
        tk::UNION => CompleteTypeObject::Union(read_complete_union(r)?),
        tk::ENUM => CompleteTypeObject::Enum(read_complete_enum(r)?),
        tk::ALIAS => CompleteTypeObject::Alias(read_complete_alias(r)?),
        tk::BITMASK => CompleteTypeObject::Bitmask(read_complete_bitmask(r)?),
        tk::BITSET => CompleteTypeObject::Bitset(read_complete_bitset(r)?),
        _ => return Err("unsupported TypeObject kind".to_string()),
    };
    Ok(obj)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_appendable() -> TypeFlag {
        TypeFlag::new(ExtensibilityKind::Appendable, false, false)
    }

    fn rt(obj: TypeObject) {
        let bytes = serialize_type_object(&obj);
        // DHEADER-vs-length invariant: outer DHEADER == payload length after it.
        let dh = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        assert_eq!(dh, bytes.len() - 4, "outer DHEADER must equal payload length");
        let back = deserialize_type_object(&bytes).expect("deserialize");
        assert_eq!(obj, back, "roundtrip mismatch");
    }

    #[test]
    fn golden_minimal_struct_long_id() {
        let mut s = MinimalStructType::new(mk_appendable(), None);
        s.add_member(MinimalStructMember::new(
            0,
            MemberFlag::default(),
            TypeIdentifier::Int32,
            "id",
        ));
        let obj = TypeObject::Minimal(MinimalTypeObject::Struct(s));
        let bytes = serialize_type_object(&obj);

        let mut expected = Vec::new();
        expected.extend_from_slice(&35u32.to_le_bytes()); // [0..4) outer DHEADER
        expected.push(EK_MINIMAL); // [4]
        expected.push(tk::STRUCTURE); // [5]
        expected.extend_from_slice(&2u16.to_le_bytes()); // [6..8) IS_APPENDABLE
        expected.extend_from_slice(&1u32.to_le_bytes()); // [8..12) header DHEADER
        expected.push(tk::NONE); // [12] TK_NONE base_type
        expected.extend_from_slice(&[0, 0, 0]); // [13..16) pad
        expected.extend_from_slice(&19u32.to_le_bytes()); // [16..20) member_seq DHEADER
        expected.extend_from_slice(&1u32.to_le_bytes()); // [20..24) count
        expected.extend_from_slice(&11u32.to_le_bytes()); // [24..28) member DHEADER
        expected.extend_from_slice(&0u32.to_le_bytes()); // [28..32) member_id
        expected.extend_from_slice(&1u16.to_le_bytes()); // [32..34) TRY_CONSTRUCT DISCARD
        expected.push(type_kind::TK_INT32); // [34] 0x04 per spec IDL
        expected.extend_from_slice(&compute_name_hash("id").to_be_bytes()); // [35..39)

        assert_eq!(bytes.len(), 39);
        assert_eq!(bytes, expected);
        rt(obj);
    }

    #[test]
    fn minimal_hash_differs_from_complete() {
        let mut m = MinimalStructType::new(mk_appendable(), None);
        m.add_member(MinimalStructMember::new(
            0,
            MemberFlag::default(),
            TypeIdentifier::Int32,
            "id",
        ));
        let mut c = CompleteStructType::new(mk_appendable(), "S".to_string(), None);
        c.add_member(CompleteStructMember::new(
            0,
            MemberFlag::default(),
            TypeIdentifier::Int32,
            "id".to_string(),
        ));
        let mh = spec_hash(&TypeObject::Minimal(MinimalTypeObject::Struct(m)));
        let ch = spec_hash(&TypeObject::Complete(CompleteTypeObject::Struct(c)));
        assert_ne!(mh, ch);
    }

    #[test]
    fn roundtrip_minimal_struct_rich() {
        let base = TypeIdentifier::MinimalTypeId(EquivalenceHash::new([9u8; 14]));
        let mut s = MinimalStructType::new(mk_appendable(), Some(base));
        s.add_member(MinimalStructMember::new(
            0,
            MemberFlag::default(),
            TypeIdentifier::String8,
            "name",
        ));
        s.add_member(MinimalStructMember::new(
            1,
            MemberFlag::default(),
            TypeIdentifier::String8Small { bound: 10 },
            "code",
        ));
        let seq = TypeIdentifier::PlainSequenceLarge {
            header: PlainCollectionHeader {
                equiv_kind: EquivalenceKind::Minimal,
                element_flags: CollectionElementFlag::default(),
            },
            bound: 0,
            element_identifier: Box::new(TypeIdentifier::MinimalTypeId(EquivalenceHash::new(
                [3u8; 14],
            ))),
        };
        s.add_member(MinimalStructMember::new(2, MemberFlag::default(), seq, "items"));
        let map = TypeIdentifier::PlainMapSmall {
            header: PlainCollectionHeader::default(),
            bound: 5,
            key_flags: CollectionElementFlag::default(),
            key_identifier: Box::new(TypeIdentifier::Int32),
            element_identifier: Box::new(TypeIdentifier::String8),
        };
        s.add_member(MinimalStructMember::new(3, MemberFlag::default(), map, "lookup"));
        let arr = TypeIdentifier::PlainArrayLarge {
            header: PlainCollectionHeader::default(),
            array_bound_seq: vec![3, 4],
            element_identifier: Box::new(TypeIdentifier::Int32),
        };
        s.add_member(MinimalStructMember::new(4, MemberFlag::default(), arr, "grid"));
        rt(TypeObject::Minimal(MinimalTypeObject::Struct(s)));
    }

    #[test]
    fn roundtrip_string8_member_stays_string8() {
        let mut s = MinimalStructType::new(mk_appendable(), None);
        s.add_member(MinimalStructMember::new(
            0,
            MemberFlag::default(),
            TypeIdentifier::String8,
            "text",
        ));
        let obj = TypeObject::Minimal(MinimalTypeObject::Struct(s));
        let bytes = serialize_type_object(&obj);
        let back = deserialize_type_object(&bytes).unwrap();
        match back {
            TypeObject::Minimal(MinimalTypeObject::Struct(sb)) => {
                assert_eq!(sb.member_seq[0].common.member_type_id, TypeIdentifier::String8)
            }
            _ => panic!("expected minimal struct"),
        }
    }

    #[test]
    fn roundtrip_union() {
        let mut u =
            MinimalUnionType::new(mk_appendable(), MemberFlag::default(), TypeIdentifier::Int32);
        u.add_member(MinimalUnionMember::new(
            0,
            MemberFlag::default(),
            TypeIdentifier::Int32,
            vec![1, 2],
            "a",
        ));
        u.add_member(MinimalUnionMember::new(
            1,
            MemberFlag::new(TryConstructKind::Discard, false, false, false, false, true),
            TypeIdentifier::String8,
            vec![],
            "b",
        ));
        rt(TypeObject::Minimal(MinimalTypeObject::Union(u)));
    }

    #[test]
    fn roundtrip_enum() {
        let mut e = MinimalEnumeratedType::new(TypeFlag::default(), 32);
        e.add_literal(MinimalEnumeratedLiteral::new(0, EnumeratedLiteralFlag::DEFAULT, "RED"));
        e.add_literal(MinimalEnumeratedLiteral::new(1, EnumeratedLiteralFlag::default(), "GREEN"));
        rt(TypeObject::Minimal(MinimalTypeObject::Enum(e)));
    }

    #[test]
    fn roundtrip_alias() {
        let a = MinimalAliasType::new(
            TypeFlag::default(),
            MemberFlag::default(),
            TypeIdentifier::Int32,
        );
        rt(TypeObject::Minimal(MinimalTypeObject::Alias(a)));
    }

    #[test]
    fn roundtrip_bitmask() {
        let mut b = MinimalBitmaskType::new(TypeFlag::default(), 8);
        b.add_flag(MinimalBitflag::new(0, MemberFlag::default(), "flag0"));
        b.add_flag(MinimalBitflag::new(3, MemberFlag::default(), "flag3"));
        rt(TypeObject::Minimal(MinimalTypeObject::Bitmask(b)));
    }

    #[test]
    fn roundtrip_bitset() {
        let mut b = MinimalBitsetType::new(TypeFlag::default());
        b.add_field(MinimalBitfield::new(0, MemberFlag::default(), 3, TypeIdentifier::Byte, "b0"));
        b.add_field(MinimalBitfield::new(
            3,
            MemberFlag::default(),
            5,
            TypeIdentifier::Uint16,
            "b1",
        ));
        rt(TypeObject::Minimal(MinimalTypeObject::Bitset(b)));
    }

    #[test]
    fn roundtrip_complete_variants() {
        let mut s = CompleteStructType::new(
            mk_appendable(),
            "MyStruct".to_string(),
            Some(TypeIdentifier::CompleteTypeId(EquivalenceHash::new([7u8; 14]))),
        );
        s.add_member(CompleteStructMember::new(
            0,
            MemberFlag::default(),
            TypeIdentifier::Int32,
            "id".to_string(),
        ));
        s.add_member(CompleteStructMember::new(
            1,
            MemberFlag::default(),
            TypeIdentifier::String8,
            "name".to_string(),
        ));
        rt(TypeObject::Complete(CompleteTypeObject::Struct(s)));

        let mut u = CompleteUnionType::new(
            mk_appendable(),
            MemberFlag::default(),
            TypeIdentifier::Int32,
            "MyUnion".to_string(),
        );
        u.add_member(CompleteUnionMember::new(
            0,
            MemberFlag::default(),
            TypeIdentifier::Int32,
            vec![7],
            "x".to_string(),
        ));
        rt(TypeObject::Complete(CompleteTypeObject::Union(u)));

        let mut e = CompleteEnumeratedType::new(TypeFlag::default(), "MyEnum".to_string(), 32);
        e.add_literal(CompleteEnumeratedLiteral::new(
            0,
            EnumeratedLiteralFlag::DEFAULT,
            "A".to_string(),
        ));
        e.add_literal(CompleteEnumeratedLiteral::new(
            1,
            EnumeratedLiteralFlag::default(),
            "B".to_string(),
        ));
        rt(TypeObject::Complete(CompleteTypeObject::Enum(e)));

        let a = CompleteAliasType::new(
            TypeFlag::default(),
            "MyAlias".to_string(),
            MemberFlag::default(),
            TypeIdentifier::Int32,
        );
        rt(TypeObject::Complete(CompleteTypeObject::Alias(a)));

        let mut bm = CompleteBitmaskType::new(TypeFlag::default(), "MyBitmask".to_string(), 8);
        bm.add_flag(CompleteBitflag::new(0, MemberFlag::default(), "f0".to_string()));
        rt(TypeObject::Complete(CompleteTypeObject::Bitmask(bm)));

        let mut bs = CompleteBitsetType::new(TypeFlag::default(), "MyBitset".to_string());
        bs.add_field(CompleteBitfield::new(
            0,
            MemberFlag::default(),
            3,
            TypeIdentifier::Byte,
            "g0".to_string(),
        ));
        rt(TypeObject::Complete(CompleteTypeObject::Bitset(bs)));
    }

    #[test]
    fn unsupported_kind_rejected() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&2u32.to_le_bytes());
        buf.push(EK_MINIMAL);
        buf.push(tk::SEQUENCE);
        assert!(deserialize_type_object(&buf).is_err());
    }

    #[test]
    fn overrunning_dheader_rejected() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&100u32.to_le_bytes());
        buf.push(EK_MINIMAL);
        assert!(deserialize_type_object(&buf).is_err());
    }
}
