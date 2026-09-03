//! Transport-generic framing plus an in-memory duplex with a bounded in-flight window, so a stalled
//! reader is observable on the writer's side exactly as a wedged pipe client would be.

use crate::protocol::Frame;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    Closed,
    /// The peer is not reading; nothing was sent.
    WouldBlock,
    /// Only the byte count is ever recorded.
    Malformed {
        bytes: usize,
    },
}

pub trait Transport {
    fn send(&mut self, frame: &Frame) -> Result<(), TransportError>;
    fn recv(&mut self) -> Result<Option<Frame>, TransportError>;
    fn close(&mut self);
    fn is_closed(&self) -> bool;
}

pub const DEFAULT_WINDOW: usize = 16;

#[derive(Default)]
struct Lane {
    frames: VecDeque<Vec<u8>>,
    closed: bool,
}

pub struct DuplexEnd {
    outbound: Rc<RefCell<Lane>>,
    inbound: Rc<RefCell<Lane>>,
    window: usize,
}

pub struct DuplexPair;

impl DuplexPair {
    pub fn pair() -> (DuplexEnd, DuplexEnd) {
        Self::with_window(DEFAULT_WINDOW)
    }
    pub fn with_window(window: usize) -> (DuplexEnd, DuplexEnd) {
        let a = Rc::new(RefCell::new(Lane::default()));
        let b = Rc::new(RefCell::new(Lane::default()));
        (
            DuplexEnd {
                outbound: a.clone(),
                inbound: b.clone(),
                window,
            },
            DuplexEnd {
                outbound: b,
                inbound: a,
                window,
            },
        )
    }
}

impl DuplexEnd {
    /// Inject raw bytes as the peer would, for malformed-frame tests.
    pub fn inject_raw(&mut self, bytes: Vec<u8>) {
        self.inbound.borrow_mut().frames.push_back(bytes);
    }
    pub fn pending(&self) -> usize {
        self.outbound.borrow().frames.len()
    }
}

impl Transport for DuplexEnd {
    fn send(&mut self, frame: &Frame) -> Result<(), TransportError> {
        let mut lane = self.outbound.borrow_mut();
        if lane.closed || self.inbound.borrow().closed {
            return Err(TransportError::Closed);
        }
        if lane.frames.len() >= self.window {
            return Err(TransportError::WouldBlock);
        }
        lane.frames
            .push_back(serde_json::to_vec(frame).expect("frame serializes"));
        Ok(())
    }
    fn recv(&mut self) -> Result<Option<Frame>, TransportError> {
        let next = self.inbound.borrow_mut().frames.pop_front();
        match next {
            None => {
                if self.inbound.borrow().closed {
                    Err(TransportError::Closed)
                } else {
                    Ok(None)
                }
            }
            Some(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| TransportError::Malformed { bytes: bytes.len() }),
        }
    }
    fn close(&mut self) {
        self.outbound.borrow_mut().closed = true;
        self.inbound.borrow_mut().closed = true;
    }
    fn is_closed(&self) -> bool {
        self.outbound.borrow().closed
    }
}
