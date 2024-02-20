use async_channel::{unbounded as async_unbounded, Sender};
use bevy::{
    prelude::*,
    tasks::{IoTaskPool, TaskPool},
};
use crossbeam_channel::{unbounded, Receiver, TryRecvError};
use js_sys::{ArrayBuffer, JsString, Uint8Array};
use tungstenite::Message;
use wasm_bindgen::prelude::*;
use web_sys::{BinaryType, ErrorEvent, MessageEvent, WebSocket};

use crate::RecvError;

pub struct WasmClientPlugin;

impl Plugin for WasmClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(WsClient);
    }
}

#[derive(Resource)]
pub struct WsClient;

impl WsClient {
    pub fn connect(
        &self,
        commands: &mut Commands,
        init_url: impl ToString,
        init_message: Option<Message>,
    ) {
        let url = init_url.to_string();
        if let Ok(ws) = WebSocket::new(&url) {
            let (tx, out_rx) = async_unbounded::<Message>();
            let (out_tx, rx) = unbounded::<Message>();

            ws.set_binary_type(BinaryType::Arraybuffer);

            // create callback
            let onmessage_callback = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
                if let Ok(buffer) = e.data().dyn_into::<ArrayBuffer>() {
                    let bytes = Uint8Array::new(&buffer).to_vec();
                    if out_tx.send(Message::Binary(bytes)).is_err() {
                        return;
                    }
                } else if let Ok(txt) = e.data().dyn_into::<JsString>() {
                    if out_tx.send(Message::Text(txt.into())).is_err() {
                        return;
                    }
                }
            });
            ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
            onmessage_callback.forget();

            let onerror_callback = Closure::<dyn FnMut(_)>::new(move |e: ErrorEvent| {
                error!("{:?}", e);
            });
            ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
            onerror_callback.forget();

            let cloned_ws = ws.clone();
            let onopen_callback = Closure::<dyn FnMut()>::new(move || match &init_message {
                Some(Message::Text(s)) => {
                    let _ = cloned_ws.send_with_str(&s);
                }
                Some(Message::Binary(bytes)) => {
                    let _ = cloned_ws.send_with_u8_array(&bytes);
                }
                Some(m) => {
                    let _ = cloned_ws.send_with_u8_array(&m.clone().into_data());
                }
                _ => {}
            });
            ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
            onopen_callback.forget();

            let cloned_ws = ws.clone();
            IoTaskPool::get_or_init(TaskPool::new)
                .spawn(async move {
                    loop {
                        match out_rx.recv().await {
                            Ok(message) => match message {
                                Message::Text(s) => {
                                    if cloned_ws.send_with_str(&s).is_err() {
                                        break;
                                    }
                                }
                                Message::Binary(bytes) => {
                                    if cloned_ws.send_with_u8_array(&bytes).is_err() {
                                        break;
                                    }
                                }
                                _ => {
                                    if cloned_ws.send_with_u8_array(&message.into_data()).is_err() {
                                        break;
                                    }
                                }
                            },
                            Err(_) => break,
                        }
                    }
                })
                .detach();

            commands.spawn(WsConnection { tx, rx });
        }
    }
}

#[derive(Component)]
pub struct WsConnection {
    tx: Sender<Message>,
    rx: Receiver<Message>,
}

impl From<TryRecvError> for RecvError {
    fn from(e: TryRecvError) -> RecvError {
        match e {
            TryRecvError::Empty => RecvError::Empty,
            TryRecvError::Disconnected => RecvError::Closed,
        }
    }
}

impl WsConnection {
    pub fn recv(&self) -> Result<Message, RecvError> {
        Ok(self.rx.try_recv()?)
    }

    pub fn send(&self, message: Message) -> bool {
        self.tx.try_send(message).is_ok()
    }

    // pub fn jsonrpc_recv(&self) -> Result<(String, Value), TryRecvError> {
    //     let res = self.recv()?;
    //     // change res to jsonrpc
    // }

    // pub fn jsonrpc_send(&self, method: &str, params: Value, is_req: bool) -> bool {
    //     // TODO change to jsonrpc
    //     let msg = Message::text();
    //     self.send(msg)
    // }
}
