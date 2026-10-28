//! How an outbound TCP connection is opened.

use std::io;
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static BIND_FAILURE_REPORTED: AtomicBool = AtomicBool::new(false);

/// Send the SYN and return at once with a non-blocking stream that is still
/// connecting. It turns writable once the peer answers either way, and only
/// its pending error then tells an accepted connection from a refused one.
#[cfg_attr(not(windows), allow(unused_variables))]
pub(crate) fn start_dial(
    addr: SocketAddr,
    bind_ip: Option<IpAddr>,
    wait: Duration,
) -> io::Result<TcpStream> {
    let socket = socket2::Socket::new(
        socket2::Domain::for_address(addr),
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )?;

    if let Some(bind_ip) = bind_ip.filter(|bind_ip| bind_ip.is_ipv4() == addr.is_ipv4()) {
        if let Err(error) = socket.bind(&SocketAddr::new(bind_ip, 0).into()) {
            if !BIND_FAILURE_REPORTED.swap(true, Ordering::Relaxed) {
                log::error!("[TcpSender] Failed to bind to {}: {}", bind_ip, error);
            }
            return Err(error);
        }
        BIND_FAILURE_REPORTED.store(false, Ordering::Relaxed);
    }
    // Windows answers a refused SYN by sending it again and reports the refusal
    // only once its retries run out, which turns a cheap refusal into a timeout
    // that earns a backoff.
    #[cfg(windows)]
    {
        let rtt_ms = wait.as_millis().clamp(MIN_INITIAL_RTO_MS as u128, MAX_INITIAL_RTO_MS as u128);
        if let Err(error) = refuse_without_syn_retransmission(&socket, rtt_ms as u16) {
            log::debug!("TCP connect to {addr} keeps retransmitting its SYN: {error}");
        }
    }
    socket.set_nonblocking(true)?;
    match socket.connect(&addr.into()) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
        #[cfg(unix)]
        Err(error) if error.raw_os_error() == Some(libc::EINPROGRESS) => {}
        Err(error) => return Err(error),
    }
    Ok(socket.into())
}

/// Wait on the calling thread for a dial started by `start_dial` to settle.
#[cfg(test)]
pub(crate) fn wait_for_dial(stream: &TcpStream, wait: Duration) -> io::Result<()> {
    let deadline = std::time::Instant::now() + wait;
    loop {
        if let Some(error) = stream.take_error()? {
            return Err(error);
        }
        match stream.peer_addr() {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotConnected => {}
            Err(error) => return Err(error),
        }
        if std::time::Instant::now() >= deadline {
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// With its one SYN never resent, a dial to a silent host ends after a single
/// initial retransmission timeout, which the kernel holds to this range, so the
/// connect timeout is honored only up to the upper bound.
#[cfg(windows)]
const MIN_INITIAL_RTO_MS: u16 = 300;
#[cfg(windows)]
const MAX_INITIAL_RTO_MS: u16 = 3000;

/// No SYN retransmission at all, as the SDK header defines it: `(UCHAR)-2`.
#[cfg(windows)]
const TCP_INITIAL_RTO_NO_SYN_RETRANSMISSIONS: u8 = 254;

#[cfg(windows)]
fn refuse_without_syn_retransmission(socket: &socket2::Socket, rtt_ms: u16) -> io::Result<()> {
    use std::os::windows::io::AsRawSocket;
    use windows_sys::Win32::Networking::WinSock::{
        WSAIoctl, SIO_TCP_INITIAL_RTO, SOCKET, TCP_INITIAL_RTO_PARAMETERS,
    };

    let params = TCP_INITIAL_RTO_PARAMETERS {
        Rtt: rtt_ms,
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
    /// so its acceptance is checked here at both ends of the timeout range.
    #[test]
    fn the_syn_retransmission_option_is_accepted() {
        for rtt_ms in [MIN_INITIAL_RTO_MS, MAX_INITIAL_RTO_MS] {
            let socket = socket2::Socket::new(
                socket2::Domain::IPV4,
                socket2::Type::STREAM,
                Some(socket2::Protocol::TCP),
            )
            .unwrap();

            refuse_without_syn_retransmission(&socket, rtt_ms)
                .unwrap_or_else(|error| panic!("the kernel rejected Rtt {rtt_ms} ms: {error}"));
        }
    }

    /// An accepted option whose meaning has changed would still retry the SYN,
    /// so the refusal must arrive well before the first retransmission could.
    #[test]
    fn a_refused_dial_is_reported_at_once() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let closed = listener.local_addr().unwrap();
        drop(listener);

        let call = Instant::now();
        let wait = Duration::from_secs(5);
        let error = start_dial(closed, None, wait)
            .and_then(|stream| wait_for_dial(&stream, wait))
            .expect_err("nothing listens there");

        assert_eq!(error.kind(), io::ErrorKind::ConnectionRefused);
        assert!(
            call.elapsed() < Duration::from_millis(250),
            "the refusal took {:?}, so the SYN was sent again",
            call.elapsed()
        );
    }
}
