use async_channel::{
    unbounded as async_unbounded, Receiver as AsyncReceiver, Sender,
    TryRecvError as AsyncTryRecvError,
};
use bevy::{
    prelude::*,
    tasks::{IoTaskPool, TaskPool},
};
use crossbeam_channel::{unbounded, Receiver, TryRecvError};
use js_sys::{ArrayBuffer, JsString, Uint8Array};
use serde_json::{json, Value};
use tungstenite::Message;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    BinaryType, ErrorEvent, Headers, MessageEvent, Request, RequestInit, Response, WebSocket,
};

use crate::RecvError;

pub struct WasmClientPlugin;

impl Plugin for WasmClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(HttpClient).insert_resource(WsClient);
    }
}

#[derive(Resource)]
pub struct HttpClient;

#[derive(Component)]
pub struct HttpConnection {
    pub method: String,
    rx: AsyncReceiver<Result<Value, Value>>,
}

impl HttpConnection {
    pub fn recv(&self) -> Result<Result<Value, Value>, RecvError> {
        Ok(self.rx.try_recv()?)
    }
}

async fn fetch_http(url: String, data: Value) -> Result<Value, Value> {
    let mut opts = RequestInit::new();
    opts.method("POST");
    let body = serde_json::to_string(&data).unwrap_or("".to_owned());
    opts.body(Some(&JsValue::from_str(&body)));
    let headers = Headers::new().unwrap();
    headers
        .set("Content-Length", &body.len().to_string())
        .unwrap();
    opts.headers(&headers);
    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| e.as_string().unwrap_or("Failure".to_owned()))?;

    let window = web_sys::window().unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| {
            info!("{:?}", e);
            "Failure".to_owned()
        })?;

    let resp: Response = resp_value
        .dyn_into()
        .map_err(|e| e.as_string().unwrap_or("Failure".to_owned()))?;

    let data: JsValue = JsFuture::from(
        resp.json()
            .map_err(|e| e.as_string().unwrap_or("Failure".to_owned()))?,
    )
    .await
    .map_err(|e| e.as_string().unwrap_or("Failure".to_owned()))?;

    let value: Value = serde_wasm_bindgen::from_value(data).map_err(|e| e.to_string())?;

    if let Some(result) = value.get("result") {
        Ok(result.clone())
    } else if let Some(error) = value.get("error") {
        Err(error.clone())
    } else {
        Err(json!("Invalid response"))
    }
}

impl HttpClient {
    pub fn jsonrpc(
        &self,
        commands: &mut Commands,
        url: &str,
        method: &str,
        gid: u64,
        params: Value,
        extra: Vec<(String, Value)>,
    ) {
        let method = method.to_owned();
        let (tx, rx) = async_unbounded::<Result<Value, Value>>();
        let mut req = json!({
            "jsonrpc": "2.0",
            "id": 0,
            "gid": gid,
            "method": method,
            "params": params
        });
        if let Some(req) = req.as_object_mut() {
            for (k, v) in extra {
                req.insert(k, v);
            }
        }

        let url = url.to_string();
        IoTaskPool::get_or_init(TaskPool::new)
            .spawn(async move {
                let _ = tx.send(fetch_http(url, req).await).await;
            })
            .detach();

        commands.spawn(HttpConnection { method, rx });
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

impl From<TryRecvError> for RecvError {
    fn from(e: TryRecvError) -> RecvError {
        match e {
            TryRecvError::Empty => RecvError::Empty,
            TryRecvError::Disconnected => RecvError::Closed,
        }
    }
}

impl From<AsyncTryRecvError> for RecvError {
    fn from(e: AsyncTryRecvError) -> RecvError {
        match e {
            AsyncTryRecvError::Empty => RecvError::Empty,
            AsyncTryRecvError::Closed => RecvError::Closed,
        }
    }
}
