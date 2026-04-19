use std::sync::mpsc::{self, Receiver, Sender, RecvTimeoutError};
use std::time::Duration;

/// イベントループで処理されるタスク。
/// 状態 T を変更するために、コンテキストのコンテキストで実行されるクロージャ。
pub type EventCallback<T> = Box<dyn FnOnce(&mut T) + Send>;

/// 汎用イベントループ
pub struct EventLoop<T> {
    receiver: Receiver<EventCallback<T>>,
    sender: Sender<EventCallback<T>>,
    closing: bool,
}

impl<T> EventLoop<T> {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            receiver,
            sender,
            closing: false,
        }
    }

    pub fn sender(&self) -> Sender<EventCallback<T>> {
        self.sender.clone()
    }

    pub fn stop(&mut self) {
        self.closing = true;
    }

    pub fn poll_events(&mut self, state: &mut T, timeout: Option<Duration>) -> bool {
        if self.closing {
            return false;
        }

        let result = match timeout {
            None => self.receiver.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(d) if d.is_zero() => self.receiver.try_recv().map_err(|e| match e {
                mpsc::TryRecvError::Empty => RecvTimeoutError::Timeout,
                mpsc::TryRecvError::Disconnected => RecvTimeoutError::Disconnected,
            }),
            Some(d) => self.receiver.recv_timeout(d),
        };

        match result {
            Ok(event) => {
                event(state);
                while let Ok(evt) = self.receiver.try_recv() {
                    evt(state);
                }
                false
            }
            Err(RecvTimeoutError::Timeout) => true,
            Err(RecvTimeoutError::Disconnected) => {
                self.closing = true;
                false
            }
        }
    }

    pub fn run(&mut self, state: &mut T) {
        while !self.closing {
            self.poll_events(state, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestState {
        count: i32,
    }

    #[test]
    fn test_event_loop_basic() {
        let mut state = TestState { count: 0 };
        let mut eloop = EventLoop::new();
        let sender = eloop.sender();

        sender.send(Box::new(|s: &mut TestState| s.count += 1)).unwrap();
        sender.send(Box::new(|s: &mut TestState| s.count += 2)).unwrap();

        eloop.poll_events(&mut state, Some(Duration::from_millis(10)));
        assert_eq!(state.count, 3);
    }

    #[test]
    fn test_event_loop_timeout() {
        let mut state = TestState { count: 0 };
        let mut eloop = EventLoop::new();
        
        let timed_out = eloop.poll_events(&mut state, Some(Duration::from_millis(1)));
        assert!(timed_out);
        assert_eq!(state.count, 0);
    }
}
