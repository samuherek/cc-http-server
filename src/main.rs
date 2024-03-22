// Uncomment this block to pass the first stage
use anyhow::anyhow;
use anyhow::Context;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

struct HttpRequest {
    path: String,
    _method: String,
    _version: String,
    headers: HashMap<String, String>,
}

impl TryFrom<&mut TcpStream> for HttpRequest {
    type Error = anyhow::Error;
    fn try_from(stream: &mut TcpStream) -> Result<Self, Self::Error> {
        let mut buffer = [0; 1024];
        let bytes_read = stream.read(&mut buffer).context("read stream into data")?;
        let request = String::from_utf8_lossy(&buffer[..bytes_read]);
        let lines = request
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>();
        let [meta, header_lines @ ..] = lines.as_slice() else {
            anyhow::bail!("Parsing lines as slice.");
        };

        let splits: Vec<_> = meta.split_whitespace().collect();
        let method = splits
            .get(0)
            .ok_or_else(|| anyhow!("Could not parse method"))?;
        let path = splits
            .get(1)
            .ok_or_else(|| anyhow!("Could not parse path"))?;
        let version = splits
            .get(2)
            .ok_or_else(|| anyhow!("Could not parse version"))?;

        let mut headers = HashMap::new();
        for line in header_lines {
            let (name, content) = line
                .split_once(": ")
                .ok_or_else(|| anyhow!("Could not parse hader value"))?;
            if name.len() > 0 && content.len() > 0 {
                headers.insert(name.to_string(), content.to_string());
            }
        }

        Ok(HttpRequest {
            path: path.to_string(),
            _method: method.to_string(),
            _version: version.to_string(),
            headers,
        })
    }
}

struct HttpResponse {
    status_code: u16,
    status_message: String,
    body: String,
}

impl HttpResponse {
    fn new(status_code: u16, status_message: &str, body: &str) -> Self {
        HttpResponse {
            status_code,
            status_message: status_message.to_string(),
            body: body.to_string(),
        }
    }

    fn to_string(&self) -> String {
        format!(
            "HTTP/1.1 {} {}\r\nContent-Length: {}\r\n\r\n{}",
            self.status_code,
            self.status_message,
            self.body.len(),
            self.body
        )
    }
}

trait RequestHandler {
    fn handle_request(&self, request: &HttpRequest) -> HttpResponse;
}

struct EchoHandler;

impl RequestHandler for EchoHandler {
    fn handle_request(&self, request: &HttpRequest) -> HttpResponse {
        let body = request.path.strip_prefix("/echo/").unwrap_or_default();
        HttpResponse::new(200, "OK", body)
    }
}

struct UserAgentHandler;

impl RequestHandler for UserAgentHandler {
    fn handle_request(&self, request: &HttpRequest) -> HttpResponse {
        let unknown = "Unknown".to_string();
        let user_agent = request.headers.get("User-Agent").unwrap_or(&unknown);
        HttpResponse::new(200, "OK", user_agent)
    }
}

struct SuccessHandler;

impl RequestHandler for SuccessHandler {
    fn handle_request(&self, _: &HttpRequest) -> HttpResponse {
        HttpResponse::new(200, "OK", "")
    }
}

struct NotFoundHandler;

impl RequestHandler for NotFoundHandler {
    fn handle_request(&self, _: &HttpRequest) -> HttpResponse {
        HttpResponse::new(404, "Not Found", "")
    }
}

fn main() -> anyhow::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();

    for stream in listener.incoming() {
        thread::spawn(move || match stream {
            Ok(mut stream) => {
                let request = HttpRequest::try_from(&mut stream).unwrap();
                let response = if request.path.starts_with("/echo") {
                    EchoHandler.handle_request(&request)
                } else if request.path == "/user-agent" {
                    UserAgentHandler.handle_request(&request)
                } else if request.path == "/" {
                    SuccessHandler.handle_request(&request)
                } else {
                    NotFoundHandler.handle_request(&request)
                };

                stream
                    .write_all(response.to_string().as_bytes())
                    .context("Write response to stream")
                    .unwrap();
            }
            Err(e) => {
                println!("error: {}", e);
            }
        });
    }

    Ok(())
}
