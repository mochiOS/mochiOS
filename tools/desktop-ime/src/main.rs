//! Host adapter for the actual input.service IME, using the native request format.
use input_service::ime::{self, InputMethod};
use std::io::{self, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::time::Duration;

fn serve(stream: &mut UnixStream, input: &mut InputMethod) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut header = [0; 4];
    stream.read_exact(&mut header)?;
    let length = u32::from_le_bytes(header) as usize;
    if !(4..=ime::MAX_REQUEST_LEN).contains(&length) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid IME request length",
        ));
    }
    let mut request = vec![0; length];
    stream.read_exact(&mut request)?;
    let reply = if ime::is_status_request(&request) {
        input.state_response()
    } else if ime::is_toggle_request(&request) {
        input.toggle();
        input.state_response()
    } else {
        input.handle(&request)
    };
    stream.write_all(&(reply.len() as u32).to_le_bytes())?;
    stream.write_all(&reply)
}

fn main() -> io::Result<()> {
    let mut arguments = std::env::args_os().skip(1);
    let dictionary = arguments
        .next()
        .ok_or_else(|| io::Error::other("missing dictionary path"))?;
    let socket = arguments
        .next()
        .ok_or_else(|| io::Error::other("missing socket path"))?;
    let mut input = InputMethod::open_dictionary(Path::new(&dictionary));
    if !input.is_available() {
        return Err(io::Error::other(
            "could not load the mochiOS IME dictionary",
        ));
    }
    let listener = UnixListener::bind(socket)?;
    eprintln!("mochiOS preview IME: native conversion engine and dictionary ready");
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let _ = serve(&mut stream, &mut input);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
