mod ws;

pub use ws::{Message, WsClient, WsClientPlugin, WsConnection};

pub enum RecvError {
    Empty,
    Closed,
}
