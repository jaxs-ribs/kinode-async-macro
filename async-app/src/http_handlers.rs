use crate::*;

pub fn my_api_handler(_state: &mut AppState, _payload: String) -> (HttpResponse, Vec<u8>) {
    (HttpResponse::new(StatusCode::OK), "".as_bytes().to_vec())
}