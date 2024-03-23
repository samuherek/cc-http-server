// Uncomment this block to pass the first stage
use anyhow::anyhow;
use anyhow::Context;
use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

struct HttpRequest {
    path: String,
    method: String,
    _version: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl TryFrom<&mut TcpStream> for HttpRequest {
    type Error = anyhow::Error;

    fn try_from(stream: &mut TcpStream) -> Result<Self, Self::Error> {
        let mut reader = BufReader::new(stream);
        let mut request_line = String::new();
        reader
            .read_line(&mut request_line)
            .context("Read the request line.")?;
        let splits: Vec<_> = request_line.split_whitespace().collect();
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
        let mut header = String::new();
        loop {
            header.clear();
            reader.read_line(&mut header).context("Read header line")?;

            if header == "\r\n" {
                break;
            }

            let (name, content) = header
                .trim()
                .split_once(": ")
                .ok_or_else(|| anyhow!("Could not parse hader value {}", header))?;
            if name.len() > 0 && content.len() > 0 {
                headers.insert(name.to_string(), content.to_string());
            }
        }

        let content_length = headers
            .get("Content-Length")
            .unwrap_or(&"0".to_string())
            .parse::<usize>()
            .unwrap_or(0);
        println!("Content lenght from request {content_length}");
        let mut body = Vec::with_capacity(content_length);
        reader.read_exact(&mut body).context("Read body")?;

        Ok(HttpRequest {
            path: path.to_string(),
            method: method.to_string(),
            _version: version.to_string(),
            headers,
            body,
        })
    }
}

struct HttpResponse {
    status_code: u16,
    headers: HashMap<String, String>,
    body: String,
}

impl HttpResponse {
    fn new(status_code: u16, headers: HashMap<String, String>, body: &str) -> Self {
        HttpResponse {
            status_code,
            headers,
            body: body.to_string(),
        }
    }

    fn to_string(&self) -> String {
        let status_message = match self.status_code {
            200 => "OK",
            201 => "Created",
            404 => "Not Found",
            _ => "Internal error",
        };
        let headers = self
            .headers
            .iter()
            .map(|(key, val)| format!("{}: {}", key, val))
            .collect::<Vec<_>>()
            .join("\r\n");

        format!(
            "HTTP/1.1 {} {}\r\n{}\r\n\r\n{}",
            self.status_code, status_message, headers, self.body
        )
    }
}

trait RequestHandler {
    fn handle_request(&self, request: &HttpRequest, dir: Arc<Option<String>>) -> HttpResponse;
}

struct EchoHandler;

impl RequestHandler for EchoHandler {
    fn handle_request(&self, request: &HttpRequest, _: Arc<Option<String>>) -> HttpResponse {
        let body = request.path.strip_prefix("/echo/").unwrap_or_default();
        let headers: HashMap<String, String> = [
            ("Content-Type".to_string(), "text/plain".to_string()),
            ("Content-Length".to_string(), body.len().to_string()),
        ]
        .into();
        HttpResponse::new(200, headers, body)
    }
}

struct UserAgentHandler;

impl RequestHandler for UserAgentHandler {
    fn handle_request(&self, request: &HttpRequest, _: Arc<Option<String>>) -> HttpResponse {
        let unknown = "Unknown".to_string();
        let user_agent = request.headers.get("User-Agent").unwrap_or(&unknown);
        let headers: HashMap<String, String> = [
            ("Content-Type".to_string(), "text/plain".to_string()),
            ("Content-Length".to_string(), user_agent.len().to_string()),
            ("User-Agent".to_string(), user_agent.to_string()),
        ]
        .into();
        HttpResponse::new(200, headers, user_agent)
    }
}

struct SuccessHandler;

impl RequestHandler for SuccessHandler {
    fn handle_request(&self, _: &HttpRequest, _: Arc<Option<String>>) -> HttpResponse {
        let headers: HashMap<String, String> =
            [("Content-Type".to_string(), "text/plain".to_string())].into();
        HttpResponse::new(200, headers, "")
    }
}

struct NotFoundHandler;

impl RequestHandler for NotFoundHandler {
    fn handle_request(&self, _: &HttpRequest, _: Arc<Option<String>>) -> HttpResponse {
        let headers: HashMap<String, String> =
            [("Content-Type".to_string(), "text/plain".to_string())].into();
        HttpResponse::new(404, headers, "")
    }
}

struct FileGetHander;

impl RequestHandler for FileGetHander {
    fn handle_request(&self, request: &HttpRequest, dir: Arc<Option<String>>) -> HttpResponse {
        let file_name = request.path.strip_prefix("/files/").unwrap_or_default();
        let fallback = "".to_string();
        let dir = dir.as_deref().unwrap_or(&fallback);
        let path = PathBuf::from(dir).join(file_name);
        let data = std::fs::read_to_string(path);

        match data {
            Ok(data) => {
                let headers: HashMap<String, String> = [
                    (
                        "Content-Type".to_string(),
                        "application/octet-stream".to_string(),
                    ),
                    ("Content-Length".to_string(), data.len().to_string()),
                ]
                .into();
                HttpResponse::new(200, headers, &data)
            }
            Err(_) => {
                let headers: HashMap<String, String> =
                    [("Content-Type".to_string(), "text/plain".to_string())].into();
                HttpResponse::new(404, headers, "")
            }
        }
    }
}

struct FilePostHander;

impl RequestHandler for FilePostHander {
    fn handle_request(&self, request: &HttpRequest, dir: Arc<Option<String>>) -> HttpResponse {
        let file_name = request.path.strip_prefix("/files/").unwrap_or_default();
        let fallback = "".to_string();
        let dir = dir.as_deref().unwrap_or(&fallback);
        let path = PathBuf::from(dir).join(file_name);

        let file = std::fs::File::create(&path).and_then(|mut f| f.write_all(&request.body));
        match file {
            Ok(_) => {
                println!("Wrote file to {}", path.display());
                let headers: HashMap<String, String> =
                    [("Content-Type".to_string(), "text/plain".to_string())].into();
                HttpResponse::new(201, headers, "")
            }
            Err(_) => {
                let headers: HashMap<String, String> =
                    [("Content-Type".to_string(), "text/plain".to_string())].into();
                HttpResponse::new(500, headers, "")
            }
        }
    }
}

fn main() -> anyhow::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();
    let args: Vec<_> = env::args().collect();
    let dir = args
        .iter()
        .position(|arg| arg == "--directory")
        .and_then(|idx| args.get(idx + 1).cloned());
    let dir = Arc::new(dir);

    for stream in listener.incoming() {
        let dir_arc = Arc::clone(&dir);
        thread::spawn(move || match stream {
            Ok(mut stream) => {
                let request = HttpRequest::try_from(&mut stream).unwrap();
                let response = if request.path.starts_with("/echo") {
                    EchoHandler.handle_request(&request, dir_arc)
                } else if request.path == "/user-agent" {
                    UserAgentHandler.handle_request(&request, dir_arc)
                } else if request.path.starts_with("/files") {
                    if request.method == "GET" {
                        FileGetHander.handle_request(&request, dir_arc)
                    } else if request.method == "POST" {
                        FilePostHander.handle_request(&request, dir_arc)
                    } else {
                        NotFoundHandler.handle_request(&request, dir_arc)
                    }
                } else if request.path == "/" {
                    SuccessHandler.handle_request(&request, dir_arc)
                } else {
                    NotFoundHandler.handle_request(&request, dir_arc)
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
