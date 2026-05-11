use mio::Token;

const TCP_TOKEN_OFFSET: usize = 10_000; // Offset added to TCP listener ports to keep them disjoint from UDP tokens.
const SHUTDOWN_TOKEN_RAW: usize = usize::MAX - 1; // Reserved token for the shutdown waker

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListenerToken {
    Udp(u16),
    Tcp(u16),
    Shutdown,
}

impl ListenerToken {
    pub(crate) fn to_mio(self) -> Token {
        match self {
            Self::Udp(port) => Token(port as usize),
            Self::Tcp(port) => Token(port as usize + TCP_TOKEN_OFFSET),
            Self::Shutdown => Token(SHUTDOWN_TOKEN_RAW),
        }
    }

    pub(crate) fn from_mio(token: Token) -> Option<Self> {
        match token.0 {
            n if n == SHUTDOWN_TOKEN_RAW => Some(Self::Shutdown),
            n if (TCP_TOKEN_OFFSET..TCP_TOKEN_OFFSET + 65_536).contains(&n) => {
                Some(Self::Tcp((n - TCP_TOKEN_OFFSET) as u16))
            }
            n if n < 65_536 => Some(Self::Udp(n as u16)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for t in [ListenerToken::Udp(7400), ListenerToken::Tcp(7400), ListenerToken::Shutdown] {
            assert_eq!(ListenerToken::from_mio(t.to_mio()), Some(t));
        }
    }
}
