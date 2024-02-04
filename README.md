# tdn-bevy
TDN plugin for Bevy game engine.

## Feature
- Support websocket client
- Support P2P network
- Latest Bevy (0.12+)

## Usage
### Websocket client
```rust
use tdn_bevy::{RecvError, WsClient, WsClientPlugin, WsConnection};

fn main() {
    App::new()
        .add_plugins(WsClientPlugin)
        .add_systems(Startup, connect_ws)
        .add_systems(Update, receive_message)
        .run();
}

fn connect_ws(mut commands: Commands, ws_client: Res<WsClient>) {
    ws_client.connect(&mut commands, "127.0.0.1:8000");
}

fn receive_message(mut commands: Commands, connections: Query<(Entity, &WsConnection)>) {
    for (entity, conn) in connections.iter() {
        loop {
            match conn.recv() {
                Ok(message) => {
                    println!("message: {}", message);
                    conn.send(message);
                }
                Err(RecvError::Empty) => break,
                Err(RecvError::Closed) => {
                    commands.entity(entity).despawn();
                    break;
                }
            }
        }
    }
}

```

### P2P network
TODO

## License

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.
