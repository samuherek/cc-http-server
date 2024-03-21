// Uncomment this block to pass the first stage
use anyhow::Context;
use std::io::{Read, Write};
use std::net::TcpListener;

fn main() -> anyhow::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let mut buffer = [0; 1024];
                stream.read(&mut buffer).context("read stream into data")?;
                let request = String::from_utf8_lossy(&buffer[..]);
                if let Some(line) = request.lines().next() {
                    match line.split_whitespace().take(2).last() {
                        Some("/") => {
                            stream
                                .write_all("HTTP/1.1 200 OK\r\n\r\n".as_bytes())
                                .context("write response to stream")?;
                        }
                        _ => {
                            stream
                                .write_all("HTTP/1.1 404 Not Found\r\n\r\n".as_bytes())
                                .context("write response to stream")?;
                        }
                    }
                }
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }

    Ok(())
}
