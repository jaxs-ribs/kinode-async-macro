use crate::*;

/// This will get triggered with a terminal request
/// For example, if you run `m our@async-app:async-app:template.os '"abc"'`
/// Then we will message the async receiver who will sleep 3s then answer.
pub fn kino_local_handler(
    _message: &Message,
    _state: &mut AppState,
    _server: &mut HttpServer,
    request: AsyncARequest,
) {
    kiprintln!("Receiver: Received request: {:?}", request);
    kiprintln!("Receiver: Sleeping for 3 seconds...");
    std::thread::sleep(std::time::Duration::from_secs(3));

    let response_body = match request {
        AsyncARequest::StepA(my_string) => {
            kiprintln!("Receiver: Handling StepA");
            AsyncAResponse::StepA(my_string.len() as i32)
        }
        AsyncARequest::StepB(i32_val) => {
            kiprintln!("Receiver: Handling StepB");
            AsyncAResponse::StepB(i32_val as u64 * 2)
        }
        AsyncARequest::StepC(u64_val) => {
            kiprintln!("Receiver: Handling StepC");
            AsyncAResponse::StepC(format!("Hello from the other side C: {}", u64_val))
        }
        AsyncARequest::Gather(_) => {
            AsyncAResponse::Gather(Ok("Hello from A".to_string()))
        },
    };

    kiprintln!("Receiver: Sending response: {:?}", response_body);
    let _ = Response::new().body(response_body).send();
}
