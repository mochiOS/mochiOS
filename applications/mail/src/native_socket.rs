//! Native TCP adapter over the existing capability-checked network.service.
//! Same application-side TLS/protocol code as Linux, including STARTTLS.
use crate::backend::{Error, Result};
use mochi_user_platform as platform;
use mochios_net_device_protocol as wire;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(1);
pub struct Socket {
    endpoint: u64,
    handle: u64,
    closed: bool,
}
fn failure() -> io::Error {
    io::Error::other("network service failure")
}
impl Socket {
    pub fn connect(host: &str, port: u16) -> Result<Self> {
        let endpoint =
            platform::process::find_by_name("network.service").map_err(|_| Error::Network)?;
        if endpoint == 0 {
            return Err(Error::Network);
        }
        let mut socket = Self {
            endpoint,
            handle: 0,
            closed: false,
        };
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let mut request = [0; 32 + wire::MAX_HOSTNAME_LEN];
        let mut reply = [0; wire::TCP_CONNECT_RESULT_LEN];
        let n = wire::encode_tcp_connect(id, host, port, 15000, &mut request)
            .map_err(|_| Error::Invalid)?;
        let len = socket
            .call(&request[..n], &mut reply)
            .map_err(|_| Error::Network)?;
        let (received, status, handle, _, actual_port) =
            wire::decode_tcp_connect_result(&reply[..len]).map_err(|_| Error::Protocol)?;
        if received != id || status != 0 || handle == 0 || actual_port != port {
            return Err(Error::Network);
        }
        socket.handle = handle;
        Ok(socket)
    }
    fn call(&self, request: &[u8], reply: &mut [u8]) -> io::Result<usize> {
        // A process with an identical name is not sufficient authority.
        let c = platform::process::thread_security_context(self.endpoint).map_err(|_| failure())?;
        if c.provenance != 1
            || c.package_id.get(..c.package_id_len as usize)
                != Some(b"org.mochios.network".as_slice())
        {
            return Err(failure());
        }
        let result = platform::ipc::call(self.endpoint, request, reply).map_err(|_| failure())?;
        let len = (result & 0xffffffff) as usize;
        if len > reply.len() {
            return Err(failure());
        }
        Ok(len)
    }
}
impl Read for Socket {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() || self.closed {
            return Ok(0);
        }
        let maximum = out.len().min(wire::MAX_TCP_IO_LEN);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let mut request = [0; wire::TCP_RECEIVE_REQUEST_LEN];
        let mut reply = [0; 40 + wire::MAX_TCP_IO_LEN];
        let n = wire::encode_tcp_receive(id, self.handle, 15000, maximum as u32, &mut request)
            .map_err(|_| failure())?;
        let len = self.call(&request[..n], &mut reply)?;
        let (received, status, closed, data) =
            wire::decode_tcp_receive_result(&reply[..len]).map_err(|_| failure())?;
        if received != id || status != 0 || data.len() > maximum {
            return Err(failure());
        }
        if data.is_empty() && !closed {
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        self.closed = closed;
        out[..data.len()].copy_from_slice(data);
        Ok(data.len())
    }
}
impl Write for Socket {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if data.is_empty() {
            return Ok(0);
        }
        let maximum = data.len().min(wire::MAX_TCP_IO_LEN);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let mut request = [0; 40 + wire::MAX_TCP_IO_LEN];
        let mut reply = [0; wire::TCP_IO_RESULT_LEN];
        let n = wire::encode_tcp_send(id, self.handle, 15000, &data[..maximum], &mut request)
            .map_err(|_| failure())?;
        let len = self.call(&request[..n], &mut reply)?;
        let (received, status, n) =
            wire::decode_tcp_io_result(wire::Opcode::TcpSendResult, &reply[..len])
                .map_err(|_| failure())?;
        if received != id || status != 0 || n == 0 || n as usize > maximum {
            return Err(failure());
        }
        Ok(n as usize)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Drop for Socket {
    fn drop(&mut self) {
        if self.handle == 0 {
            return;
        }
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let mut request = [0; wire::TCP_CLOSE_REQUEST_LEN];
        let mut reply = [0; wire::TCP_IO_RESULT_LEN];
        if let Ok(n) = wire::encode_tcp_close(id, self.handle, 1000, &mut request) {
            let _ = self.call(&request[..n], &mut reply);
        }
    }
}
