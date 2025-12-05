/// Wide character type for DDS (UTF-16 encoded)
///
/// WChar wraps a Rust `char` and serializes it as UTF-16 (2 bytes).
/// Note: Characters outside BMP (> U+FFFF) will be truncated during serialization.
///
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WChar {
    inner: char,
}

impl Default for WChar {
    fn default() -> Self {
        Self { inner: '\0' }
    }
}

impl WChar {
    /// Create a new WChar from a char
    pub fn new(c: char) -> Self {
        Self { inner: c }
    }

    /// Get the inner char
    pub fn as_char(&self) -> char {
        self.inner
    }

    /// Convert to UTF-16 code unit (truncates if > U+FFFF)
    pub fn to_utf16(&self) -> u16 {
        let code = self.inner as u32;
        if code > 0xFFFF {
            0xFFFD // Replacement character
        } else {
            code as u16
        }
    }

    /// Create from UTF-16 code unit
    pub fn from_utf16(code: u16) -> Self {
        Self { inner: char::from_u32(code as u32).unwrap_or('\u{FFFD}') }
    }
}

impl From<char> for WChar {
    fn from(c: char) -> Self {
        Self::new(c)
    }
}

impl From<WChar> for char {
    fn from(wc: WChar) -> Self {
        wc.inner
    }
}

impl std::ops::Deref for WChar {
    type Target = char;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::fmt::Display for WChar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

// Speedy serialization support for WChar
impl<'a, C: speedy::Context> speedy::Readable<'a, C> for WChar {
    #[inline]
    fn read_from<R: speedy::Reader<'a, C>>(reader: &mut R) -> Result<Self, C::Error> {
        let c = <char as speedy::Readable<'a, C>>::read_from(reader)?;
        Ok(WChar::from(c))
    }

    #[inline]
    fn minimum_bytes_needed() -> usize {
        <char as speedy::Readable<'a, C>>::minimum_bytes_needed()
    }
}

impl<C: speedy::Context> speedy::Writable<C> for WChar {
    #[inline]
    fn write_to<T: ?Sized + speedy::Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        <char as speedy::Writable<C>>::write_to(&self.inner, writer)
    }

    #[inline]
    fn bytes_needed(&self) -> Result<usize, C::Error> {
        <char as speedy::Writable<C>>::bytes_needed(&self.inner)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct WString {
    inner: String,
}

impl WString {
    /// Create a new WString from a String
    pub fn new(s: String) -> Self {
        Self { inner: s }
    }

    /// Create a new WString from a &str
    pub fn from_str(s: &str) -> Result<Self, std::string::FromUtf8Error> {
        Ok(Self { inner: s.to_string() })
    }

    /// Create a WString from UTF-16 code units
    pub fn from_utf16(v: &[u16]) -> Result<Self, std::string::FromUtf16Error> {
        String::from_utf16(v).map(|s| Self { inner: s })
    }

    /// Get the inner string as a reference
    pub fn as_str(&self) -> &str {
        &self.inner
    }

    /// Get the length in UTF-8 bytes
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if the string is empty
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Encode as UTF-16
    pub fn encode_utf16(&self) -> std::vec::Vec<u16> {
        self.inner.encode_utf16().collect()
    }

    /// Convert into the inner String
    pub fn into_inner(self) -> String {
        self.inner
    }
}

impl std::ops::Deref for WString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl AsRef<str> for WString {
    fn as_ref(&self) -> &str {
        &self.inner
    }
}

impl std::fmt::Display for WString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl From<String> for WString {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for WString {
    fn from(s: &str) -> Self {
        Self { inner: s.to_string() }
    }
}

impl TryFrom<Vec<u16>> for WString {
    type Error = std::string::FromUtf16Error;

    fn try_from(v: Vec<u16>) -> Result<Self, Self::Error> {
        Self::from_utf16(&v)
    }
}

impl From<WString> for String {
    fn from(ws: WString) -> Self {
        ws.inner
    }
}

// Speedy serialization support for WString
impl<'a, C: speedy::Context> speedy::Readable<'a, C> for WString {
    #[inline]
    fn read_from<R: speedy::Reader<'a, C>>(reader: &mut R) -> Result<Self, C::Error> {
        let s = <String as speedy::Readable<'a, C>>::read_from(reader)?;
        Ok(WString::from(s))
    }

    #[inline]
    fn minimum_bytes_needed() -> usize {
        <String as speedy::Readable<'a, C>>::minimum_bytes_needed()
    }
}

impl<C: speedy::Context> speedy::Writable<C> for WString {
    #[inline]
    fn write_to<T: ?Sized + speedy::Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        <String as speedy::Writable<C>>::write_to(&self.inner, writer)
    }

    #[inline]
    fn bytes_needed(&self) -> Result<usize, C::Error> {
        <String as speedy::Writable<C>>::bytes_needed(&self.inner)
    }
}
