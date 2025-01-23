use crate::*;

pub fn http_handler(_state: &mut AppState, _payload: String) -> (HttpResponse, Vec<u8>) {
    (HttpResponse::new(StatusCode::OK), "".as_bytes().to_vec())
}

pub fn ws_handler(
    _state: &mut AppState,
    _server: &mut HttpServer,
    _channel_id: u32,
    _msg_type: WsMessageType,
    _blob: LazyLoadBlob,
) {
    // no-op
    
}
