use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::io::{BufReader, BufRead, Write, Read};
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
        
        let tx_clone = tx.clone();
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
                            // Read the empty line \r\n
                            let mut empty = String::new();
                            let _ = reader.read_line(&mut empty);
                            
                            let mut buf = vec![0; len];
                            if reader.read_exact(&mut buf).is_ok() {
                                if let Ok(json) = serde_json::from_slice::<Value>(&buf) {
                                    let j = json.clone();
                                    
                                    // Handle 'initialize' response to send 'initialized'
                                    if j.get("id").and_then(|id| id.as_i64()) == Some(1) && j.get("result").is_some() {
                                        let msg = json!({
                                            "jsonrpc": "2.0",
                                            "method": "initialized",
                                            "params": {}
                                        });
                                        let _ = tx_clone.send(msg);
                                    }

                                    let _ = event_sender.send(Box::new(move |state| {
                                        if let Some(method) = j.get("method").and_then(|v| v.as_str()) {
                                            if method == "textDocument/publishDiagnostics" {
                                                if let Some(params) = j.get("params") {
                                                    if let (Some(uri), Some(diagnostics)) = (params.get("uri").and_then(|u| u.as_str()), params.get("diagnostics").and_then(|d| d.as_array())) {
                                                        // Extract file path from URI (simplistic for file://)
                                                        let path = if uri.starts_with("file://") { &uri[7..] } else { uri };
                                                        
                                                        // Find buffer with this name
                                                        for buf in &state.buffers {
                                                            let mut b = buf.borrow_mut();
                                                            if b.name().map_or(false, |n| n == path || path.ends_with(n)) {
                                                                b.virtual_text.clear(); // Clear old diagnostics
                                                                for diag in diagnostics {
                                                                    if let (Some(range), Some(message)) = (diag.get("range"), diag.get("message").and_then(|m| m.as_str())) {
                                                                        if let Some(start_line) = range.get("start").and_then(|s| s.get("line")).and_then(|l| l.as_u64()) {
                                                                            // LSP lines are 0-indexed, our buffer is 1-indexed
                                                                            let lnum = (start_line + 1) as usize;
                                                                            b.virtual_text.insert(lnum, format!("■ {}", message));
                                                                        }
                                                                    }
                                                                }
                                                                break;
                                                            }
                                                        }
                                                    }
                                                }
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

    pub fn send_initialize(&self, root_uri: &str) {
        let msg = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": root_uri,
                "capabilities": {
                    "textDocument": {
                        "synchronization": {
                            "dynamicRegistration": true,
                            "willSave": true,
                            "willSaveWaitUntil": true,
                            "didSave": true
                        }
                    }
                }
            }
        });
        let _ = self.tx.send(msg);
    }

    pub fn send_did_open(&self, uri: &str, language_id: &str, text: &str) {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": 1,
                    "text": text
                }
            }
        });
        let _ = self.tx.send(msg);
    }

    pub fn send_did_change(&self, uri: &str, version: u64, text: &str) {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "version": version
                },
                "contentChanges": [
                    {
                        "text": text
                    }
                ]
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
        client.send_initialize("file:///dummy");
    }
}
