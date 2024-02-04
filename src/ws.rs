use async_channel::{unbounded, Receiver, RecvError as ChannelRecvError, Sender, TryRecvError};
use async_tungstenite::{async_std::connect_async, tungstenite::Message};
use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use futures_lite::future::race;
use futures_util::{SinkExt, StreamExt};

use crate::RecvError;

pub struct WsClientPlugin;

impl Plugin for WsClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(WsClient);
    }
}

#[derive(Resource)]
pub struct WsClient;

enum FutureRes {
    Io(Option<Message>),
    Out(Message),
}

impl WsClient {
    pub fn connect(
        &self,
        commands: &mut Commands,
        init_url: impl ToString,
        init_message: Option<Message>,
    ) {
        let url = init_url.to_string();
        let (tx, io_rx) = unbounded::<Message>();
        let (io_tx, rx) = unbounded::<Message>();

        let task = IoTaskPool::get().spawn(async move {
            if let Ok((mut stream, _)) = connect_async(url.to_string()).await {
                if let Some(msg) = init_message {
                    let _ = stream.send(msg).await;
                }

                loop {
                    match race(
                        async { io_rx.recv().await.map(|v| FutureRes::Out(v)) },
                        async {
                            stream
                                .next()
                                .await
                                .map(|v| FutureRes::Io(v.ok()))
                                .ok_or(ChannelRecvError)
                        },
                    )
                    .await
                    {
                        Ok(FutureRes::Io(Some(v))) => {
                            let _ = io_tx.send(v).await;
                        }
                        Ok(FutureRes::Out(v)) => {
                            let _ = stream.send(v).await;
                        }
                        _ => break,
                    }
                }
            }
        });

        commands.spawn(WsConnection { _io: task, tx, rx });
    }
}

#[derive(Component)]
pub struct WsConnection {
    _io: Task<()>,
    tx: Sender<Message>,
    rx: Receiver<Message>,
}

impl From<TryRecvError> for RecvError {
    fn from(e: TryRecvError) -> RecvError {
        match e {
            TryRecvError::Empty => RecvError::Empty,
            TryRecvError::Closed => RecvError::Closed,
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
