use crate::*;

pub fn my_remote_request(
    _message: &Message,
    _state: &mut AppState,
    _server: &mut HttpServer,
    _request: String,
) {
    kiprintln!("Hi2");
}