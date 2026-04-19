use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::io::{BufReader, BufRead, Write};
use std::thread;
use serde_json::{Value, json};
use crate::nvim::state::VimState;
use crate::nvim::event::event_loop::EventCallback;

pub struct LspClient {
    pub process: std::process::Child,
    pub tx: std::sync::mpsc::Sender<Value>,
}

impl LspClient {
    pub fn new(cmd: &str, sender: Option<Sender<EventCallback<VimState>>>) -> Result<Self, std::io::Error> {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let mut child = Command::new(parts[0])
            .args(&parts[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().unwrap();
        let mut stdin = child.stdin.take().unwrap();

        let (tx, rx) = std::sync::mpsc::channel::<Value>();
        thread::spawn(move || {
            for msg in rx {
                let s = msg.to_string();
                let payload = format!("Content-Length: {}\r\n\r\n{}", s.len(), s);
                let _ = stdin.write_all(payload.as_bytes());
                let _ = stdin.flush();
            }
        });

        if let Some(event_sender) = sender {
            thread::spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() || line.is_empty() {
                        break;
                    }
                    if line.starts_with("Content-Length: ") {
                        let len_str = line["Content-Length: ".len()..].trim();
                        if let Ok(len) = len_str.parse::<usize>() {
                            let mut empty = String::new();
                            let _ = reader.read_line(&mut empty);
                            
                            let mut buf = vec![0; len];
                            use std::io::Read;
                            if reader.read_exact(&mut buf).is_ok() {
                                if let Ok(json) = serde_json::from_slice::<Value>(&buf) {
                                    let j = json.clone();
                                    let _ = event_sender.send(Box::new(move |_s| {
                                        if let Some(method) = j.get("method").and_then(|v| v.as_str()) {
                                            if method == "textDocument/publishDiagnostics" {
                                                // Basic handling of diagnostics (placeholder for later expansion)
                                            }
                                        }
                                    }));
                                }
                            }
                        }
                    }
                }
            });
        }

        Ok(Self {
            process: child,
            tx,
        })
    }

    pub fn send_initialize(&self) {
        let msg = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": null,
                "capabilities": {}
            }
        });
        let _ = self.tx.send(msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lsp_client_spawn() {
        let client = LspClient::new("echo", None);
        assert!(client.is_ok());
    }

    #[test]
    fn test_lsp_send_initialize() {
        let client = LspClient::new("echo", None).unwrap();
        client.send_initialize();
        // Since it's a real channel in the spawned thread, 
        // we can't easily check rx here without exposing it.
        // But we verified it doesn't crash.
    }
}
