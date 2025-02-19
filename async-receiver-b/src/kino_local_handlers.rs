use crate::*;

/// This will get triggered with a terminal request
/// For example, if you run `m our@async-app:async-app:template.os '"abc"'`
/// Then we will message the async receiver who will sleep 3s then answer.
pub fn kino_local_handler(
    _message: &Message,
    _state: &mut AppState,
    _server: &mut HttpServer,
    request: AsyncRequest,
) {
    let response = match request {
        AsyncRequest::StepA(s) => AsyncResponse::StepA(s.len() as i32),
        AsyncRequest::StepB(n) => AsyncResponse::StepB(n * 2),
        AsyncRequest::StepC(n) => AsyncResponse::StepC(format!("Number was: {}", n)),
        AsyncRequest::Gather(_) => AsyncResponse::Gather("Hello from receiver B".to_string()),
    };

    let _ = Response::new().body(response).send();
}
