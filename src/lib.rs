mod ws;

pub use ws::{WsClient, WsClientPlugin, WsConnection};

pub enum RecvError {
    Empty,
    Closed,
}
