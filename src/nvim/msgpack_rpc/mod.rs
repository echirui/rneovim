use crate::nvim::error::Result;
use crate::nvim::state::VimState;
use crate::nvim::event::event_loop::EventCallback;
use std::net::TcpListener;
use std::sync::mpsc::Sender;
use std::thread;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum RpcMessage {
    Request(u32, u32, String, Vec<serde_json::Value>), // type(0), msgid, method, params
    Response(u32, u32, serde_json::Value, serde_json::Value), // type(1), msgid, error, result
    Notification(u32, String, Vec<serde_json::Value>), // type(2), method, params
}

pub struct RpcServer {
    port: u16,
}

impl RpcServer {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    pub fn start(&self, sender: Sender<EventCallback<VimState>>) -> Result<()> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", self.port))?;
        thread::spawn(move || {
            for stream in listener.incoming() {
                if let Ok(stream) = stream {
                    let sender_clone = sender.clone();
                    thread::spawn(move || {
                        let _ = handle_client(stream, sender_clone);
                    });
                }
            }
        });
        Ok(())
    }
}

fn handle_client<R: std::io::Read>(mut reader: R, sender: Sender<EventCallback<VimState>>) -> std::io::Result<()> {
    if let Ok(msg) = rmp_serde::from_read::<_, RpcMessage>(&mut reader) {
        match msg {
            RpcMessage::Request(_, msgid, method, _params) => {
                let task: EventCallback<VimState> = Box::new(move |_state| {
                    println!("RPC Request: {} (id: {})", method, msgid);
                });
                let _ = sender.send(task);
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn test_handle_client_mock() {
        let (tx, rx) = mpsc::channel();
        let msg = RpcMessage::Request(0, 1, "test_method".to_string(), vec![]);
        let data = rmp_serde::to_vec(&msg).unwrap();
        
        handle_client(&data[..], tx).unwrap();
        
        let callback = rx.try_recv().unwrap();
        let mut state = VimState::new();
        callback(&mut state);
    }

    #[test]
    fn test_rpc_message_serialization() {
        let msg = RpcMessage::Request(0, 123, "nvim_get_current_buf".to_string(), vec![]);
        let serialized = rmp_serde::to_vec(&msg).unwrap();
        let deserialized: RpcMessage = rmp_serde::from_slice(&serialized).unwrap();
        
        if let RpcMessage::Request(_, id, method, _) = deserialized {
            assert_eq!(id, 123);
            assert_eq!(method, "nvim_get_current_buf");
        } else {
            panic!("Wrong message type");
        }
    }

    #[test]
    fn test_rpc_notification_serialization() {
        let msg = RpcMessage::Notification(2, "test_event".to_string(), vec![serde_json::Value::String("data".to_string())]);
        let serialized = rmp_serde::to_vec(&msg).unwrap();
        let deserialized: RpcMessage = rmp_serde::from_slice(&serialized).unwrap();
        
        if let RpcMessage::Notification(_, method, params) = deserialized {
            assert_eq!(method, "test_event");
            assert_eq!(params.len(), 1);
        } else {
            panic!("Wrong message type");
        }
    }

    #[test]
    fn test_rpc_response_serialization() {
        let msg = RpcMessage::Response(1, 456, serde_json::Value::Null, serde_json::Value::String("result".to_string()));
        let serialized = rmp_serde::to_vec(&msg).unwrap();
        let deserialized: RpcMessage = rmp_serde::from_slice(&serialized).unwrap();
        
        if let RpcMessage::Response(_, id, _error, result) = deserialized {
            assert_eq!(id, 456);
            assert_eq!(result, serde_json::Value::String("result".to_string()));
        } else {
            panic!("Wrong message type");
        }
    }
}
