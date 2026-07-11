use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::{Duration, Instant};

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
            let deadline = Instant::now() + Duration::from_secs(2);
            for response in raw_responses {
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
                stream
                    .set_read_timeout(Some(Duration::from_millis(200)))
                    .expect("read timeout");
                let mut bytes = [0_u8; 8192];
                let size = stream.read(&mut bytes).unwrap_or(0);
                sender
                    .send(String::from_utf8_lossy(&bytes[..size]).to_string())
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
