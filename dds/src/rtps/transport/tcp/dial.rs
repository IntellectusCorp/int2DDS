//! How an outbound TCP connection is opened.

use std::io;
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

#[cfg(not(windows))]
pub(crate) fn dial(addr: SocketAddr, wait: Duration) -> io::Result<TcpStream> {
    TcpStream::connect_timeout(&addr, wait)
}

/// The kernel there answers a refused SYN by sending it again and reports the
/// refusal only once its retries run out, which outlasts the connect timeout
/// and turns a cheap refusal into a timeout that earns a backoff.
#[cfg(windows)]
pub(crate) fn dial(addr: SocketAddr, wait: Duration) -> io::Result<TcpStream> {
    let socket = socket2::Socket::new(
        socket2::Domain::for_address(addr),
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )?;
    if let Err(error) = refuse_without_syn_retransmission(&socket) {
        log::debug!("TCP connect to {addr} keeps retransmitting its SYN: {error}");
    }
    socket.connect_timeout(&addr.into(), wait)?;
    Ok(socket.into())
}

/// Keeps the system's initial retransmission timeout.
#[cfg(windows)]
const TCP_INITIAL_RTO_UNSPECIFIED_RTT: u16 = 0xFFFF;

/// No SYN retransmission at all, as the SDK header defines it: `(UCHAR)-2`.
#[cfg(windows)]
const TCP_INITIAL_RTO_NO_SYN_RETRANSMISSIONS: u8 = 254;

#[cfg(windows)]
fn refuse_without_syn_retransmission(socket: &socket2::Socket) -> io::Result<()> {
    use std::os::windows::io::AsRawSocket;
    use windows_sys::Win32::Networking::WinSock::{
        WSAIoctl, SIO_TCP_INITIAL_RTO, SOCKET, TCP_INITIAL_RTO_PARAMETERS,
    };

    let params = TCP_INITIAL_RTO_PARAMETERS {
        Rtt: TCP_INITIAL_RTO_UNSPECIFIED_RTT,
        MaxSynRetransmissions: TCP_INITIAL_RTO_NO_SYN_RETRANSMISSIONS,
    };
    let mut returned = 0u32;
    // SAFETY: a socket this function's caller owns and has not connected yet,
    // an input buffer of exactly the parameter size, no output, synchronous.
    let status = unsafe {
        WSAIoctl(
            socket.as_raw_socket() as SOCKET,
            SIO_TCP_INITIAL_RTO,
            &params as *const TCP_INITIAL_RTO_PARAMETERS as *const std::ffi::c_void,
            std::mem::size_of::<TCP_INITIAL_RTO_PARAMETERS>() as u32,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
            None,
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::time::Instant;

    /// The value written out by hand must stay the one the SDK defines, in a
    /// parameter block of the size the ioctl expects.
    #[test]
    fn the_no_syn_retransmission_value_matches_the_sdk() {
        use windows_sys::Win32::Networking::WinSock as winsock;

        assert_eq!(
            winsock::TCP_INITIAL_RTO_NO_SYN_RETRANSMISSIONS as u8,
            TCP_INITIAL_RTO_NO_SYN_RETRANSMISSIONS
        );
        assert_eq!(std::mem::size_of::<winsock::TCP_INITIAL_RTO_PARAMETERS>(), 4);
    }

    /// A rejected option leaves the dial on the slow refusal without a word,
    /// so its acceptance is checked here instead.
    #[test]
    fn the_syn_retransmission_option_is_accepted() {
        let socket = socket2::Socket::new(
            socket2::Domain::IPV4,
            socket2::Type::STREAM,
            Some(socket2::Protocol::TCP),
        )
        .unwrap();

        refuse_without_syn_retransmission(&socket).expect("the kernel rejected the option");
    }

    /// An accepted option whose meaning has changed would still retry the SYN,
    /// so the refusal must arrive well before the first retransmission could.
    #[test]
    fn a_refused_dial_is_reported_at_once() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let closed = listener.local_addr().unwrap();
        drop(listener);

        let call = Instant::now();
        let error = dial(closed, Duration::from_secs(5)).expect_err("nothing listens there");

        assert_eq!(error.kind(), io::ErrorKind::ConnectionRefused);
        assert!(
            call.elapsed() < Duration::from_millis(250),
            "the refusal took {:?}, so the SYN was sent again",
            call.elapsed()
        );
    }
}
