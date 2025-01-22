use crate::*;

pub fn kino_remote_handler(
    _message: &Message,
    _state: &mut AppState,
    _server: &mut HttpServer,
    _request: String,
) {
    kiprintln!("Hi2");
}