use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const FIXTURE_IO_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_FIXTURE_REQUEST_BYTES: usize = 8192;

fn read_fixture_request(stream: &mut TcpStream) -> String {
    read_fixture_request_with_observer(stream, |_, _| {})
}

fn read_fixture_request_with_observer(
    stream: &mut TcpStream,
    observer: impl FnMut(&[u8], Option<usize>),
) -> String {
    read_fixture_request_with_timeout_and_observer(stream, FIXTURE_IO_TIMEOUT, observer)
}

fn read_fixture_request_with_timeout_and_observer(
    stream: &mut TcpStream,
    timeout: Duration,
    mut observer: impl FnMut(&[u8], Option<usize>),
) -> String {
    stream
        .set_nonblocking(false)
        .expect("set fixture stream blocking");
    let deadline = Instant::now()
        .checked_add(timeout)
        .expect("fixture request deadline overflow");
    let mut bytes = [0_u8; MAX_FIXTURE_REQUEST_BYTES];
    let mut size = 0;

    loop {
        assert!(
            size < bytes.len(),
            "fixture request exceeds {MAX_FIXTURE_REQUEST_BYTES} bytes"
        );
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .expect("fixture request deadline exceeded");
        stream
            .set_read_timeout(Some(remaining))
            .expect("set fixture request read timeout");
        let read = stream.read(&mut bytes[size..]).unwrap_or_else(|error| {
            if matches!(
                error.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) {
                panic!("fixture request deadline exceeded: {error}");
            }
            panic!("read fixture request: {error}");
        });
        assert!(read > 0, "fixture request ended before it was complete");
        size += read;

        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut request = httparse::Request::new(&mut headers);
        let header_end = match request
            .parse(&bytes[..size])
            .expect("parse fixture request headers")
        {
            httparse::Status::Partial => {
                observer(&bytes[..size], None);
                continue;
            }
            httparse::Status::Complete(header_end) => header_end,
        };
        if request
            .headers
            .iter()
            .any(|header| header.name.eq_ignore_ascii_case("transfer-encoding"))
        {
            panic!("fixture transfer-encoding is unsupported");
        }
        let content_length = request
            .headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case("content-length"))
            .map(|header| {
                std::str::from_utf8(header.value)
                    .expect("fixture content-length is UTF-8")
                    .parse::<usize>()
                    .expect("fixture content-length is valid")
            })
            .unwrap_or(0);
        let request_end = header_end
            .checked_add(content_length)
            .expect("fixture request length overflow");
        assert!(
            request_end <= bytes.len(),
            "fixture request exceeds {MAX_FIXTURE_REQUEST_BYTES} bytes"
        );
        observer(&bytes[..size], Some(request_end));
        if size < request_end {
            continue;
        }

        return String::from_utf8(bytes[..request_end].to_vec())
            .expect("fixture request is valid UTF-8");
    }
}

pub(super) struct TestHttpServer {
    pub base_url: String,
    pub requests: std::sync::mpsc::Receiver<String>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl TestHttpServer {
    pub fn sequence(raw_responses: Vec<Option<String>>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture server");
        listener
            .set_nonblocking(true)
            .expect("nonblocking fixture server");
        let address = listener.local_addr().expect("fixture address");
        let (sender, requests) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            for response in raw_responses {
                let deadline = Instant::now() + FIXTURE_IO_TIMEOUT;
                let (mut stream, _) = loop {
                    match listener.accept() {
                        Ok(accepted) => break accepted,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() >= deadline =>
                        {
                            return;
                        }
                        Err(error) => panic!("fixture accept failed: {error}"),
                    }
                };
                sender
                    .send(read_fixture_request(&mut stream))
                    .expect("capture request");
                if let Some(response) = response {
                    stream
                        .write_all(response.as_bytes())
                        .expect("write fixture response");
                }
            }
        });
        Self {
            base_url: format!("http://{address}"),
            requests,
            handle: Some(handle),
        }
    }

    pub fn finish(mut self) -> Vec<String> {
        self.handle
            .take()
            .expect("fixture handle")
            .join()
            .expect("fixture thread");
        self.requests.try_iter().collect()
    }
}

pub(super) fn json_response(status: u16, body: serde_json::Value) -> String {
    let body = body.to_string();
    let reason = if status == 200 { "OK" } else { "ERROR" };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
        if let Some(message) = panic.downcast_ref::<&str>() {
            (*message).to_string()
        } else if let Some(message) = panic.downcast_ref::<String>() {
            message.clone()
        } else {
            "non-string panic".to_string()
        }
    }

    #[test]
    fn request_reader_waits_for_bytes_after_nonblocking_accept() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture listener");
        let address = listener.local_addr().expect("fixture listener address");
        let (ready_sender, ready_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let (headers_sent_sender, headers_sent_receiver) = mpsc::channel();
        let (body_release_sender, body_release_receiver) = mpsc::channel();
        let headers =
            "POST /ready HTTP/1.1\r\nHost: localhost\r\nContent-Length: 5\r\nConnection: close\r\n\r\n";
        let body = "ready";
        let request = format!("{headers}{body}");

        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(address).expect("connect fixture client");
            ready_sender.send(()).expect("signal fixture client ready");
            release_receiver
                .recv_timeout(FIXTURE_IO_TIMEOUT)
                .expect("wait for fixture client release");
            stream
                .write_all(headers.as_bytes())
                .expect("write fixture request headers");
            headers_sent_sender
                .send(())
                .expect("signal fixture request headers sent");
            body_release_receiver
                .recv_timeout(FIXTURE_IO_TIMEOUT)
                .expect("wait for fixture request body release");
            stream
                .write_all(body.as_bytes())
                .expect("write fixture request body");
        });

        ready_receiver
            .recv_timeout(FIXTURE_IO_TIMEOUT)
            .expect("wait for fixture client ready");
        let (mut stream, _) = listener.accept().expect("accept fixture client");
        stream
            .set_nonblocking(true)
            .expect("set accepted fixture stream nonblocking");
        let mut byte = [0_u8; 1];
        let error = stream
            .peek(&mut byte)
            .expect_err("fixture request bytes must not be available yet");
        assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);

        release_sender
            .send(())
            .expect("release fixture client request");
        headers_sent_receiver
            .recv_timeout(FIXTURE_IO_TIMEOUT)
            .expect("wait for fixture request headers");
        let mut body_release_sender = Some(body_release_sender);
        let captured = read_fixture_request_with_observer(&mut stream, |buffered, request_end| {
            if request_end.is_some_and(|request_end| buffered.len() < request_end) {
                body_release_sender
                    .take()
                    .expect("release fixture request body once")
                    .send(())
                    .expect("release fixture request body");
            }
        });
        client.join().expect("fixture client thread");

        assert_eq!(captured, request);
    }

    #[test]
    fn request_reader_enforces_one_total_request_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture listener");
        let address = listener.local_addr().expect("fixture listener address");
        let (ready_sender, ready_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let headers =
            "POST /slow HTTP/1.1\r\nHost: localhost\r\nContent-Length: 8\r\nConnection: close\r\n\r\n";

        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(address).expect("connect fixture client");
            ready_sender.send(()).expect("signal fixture client ready");
            release_receiver
                .recv_timeout(FIXTURE_IO_TIMEOUT)
                .expect("wait for fixture client release");
            stream
                .write_all(headers.as_bytes())
                .expect("write slow fixture request headers");
            for byte in b"slowbody" {
                std::thread::sleep(Duration::from_millis(20));
                if stream.write_all(&[*byte]).is_err() {
                    break;
                }
            }
        });

        ready_receiver
            .recv_timeout(FIXTURE_IO_TIMEOUT)
            .expect("wait for fixture client ready");
        let (mut stream, _) = listener.accept().expect("accept fixture client");
        release_sender
            .send(())
            .expect("release slow fixture client request");

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            read_fixture_request_with_timeout_and_observer(
                &mut stream,
                Duration::from_millis(60),
                |_, _| {},
            )
        }));
        client.join().expect("slow fixture client thread");

        let message = panic_message(
            result.expect_err("fixture request reader must enforce one total deadline"),
        );
        assert!(
            message.contains("fixture request deadline exceeded"),
            "unexpected panic: {message}"
        );
    }

    #[test]
    fn request_reader_rejects_transfer_encoding() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture listener");
        let address = listener.local_addr().expect("fixture listener address");
        let (ready_sender, ready_receiver) = mpsc::channel();
        let request =
            b"POST /chunked HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n1\r\nx\r\n0\r\n\r\n";

        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(address).expect("connect fixture client");
            stream.write_all(request).expect("write chunked request");
            ready_sender.send(()).expect("signal fixture client ready");
        });

        ready_receiver
            .recv_timeout(FIXTURE_IO_TIMEOUT)
            .expect("wait for fixture client ready");
        let (mut stream, _) = listener.accept().expect("accept fixture client");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            read_fixture_request(&mut stream)
        }));
        client.join().expect("chunked fixture client thread");

        let message = panic_message(result.expect_err("transfer-encoding must be rejected"));
        assert!(
            message.contains("fixture transfer-encoding is unsupported"),
            "unexpected panic: {message}"
        );
    }

    #[test]
    fn request_reader_rejects_invalid_utf8_in_exact_capture() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture listener");
        let address = listener.local_addr().expect("fixture listener address");
        let (ready_sender, ready_receiver) = mpsc::channel();

        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(address).expect("connect fixture client");
            stream
                .write_all(
                    b"POST /invalid HTTP/1.1\r\nHost: localhost\r\nContent-Length: 1\r\n\r\n\xff",
                )
                .expect("write invalid UTF-8 fixture request");
            ready_sender.send(()).expect("signal fixture client ready");
        });

        ready_receiver
            .recv_timeout(FIXTURE_IO_TIMEOUT)
            .expect("wait for fixture client ready");
        let (mut stream, _) = listener.accept().expect("accept fixture client");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            read_fixture_request(&mut stream)
        }));
        client.join().expect("invalid UTF-8 fixture client thread");

        let message = panic_message(result.expect_err("invalid UTF-8 must be rejected"));
        assert!(
            message.contains("fixture request is valid UTF-8"),
            "unexpected panic: {message}"
        );
    }
}
