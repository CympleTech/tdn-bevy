#[cfg(feature = "ws")]
pub mod ws;

#[cfg(feature = "p2p")]
pub mod p2p;

#[cfg(feature = "wasm")]
pub mod wasm;

pub enum RecvError {
    Empty,
    Closed,
}

#[cfg(any(feature = "ws", feature = "wasm"))]
pub use tungstenite::Message;
