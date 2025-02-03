use crate::*;

/// This will get triggered with a terminal request
/// For example, if you run `m our@async-app:async-app:template.os '"abc"'`
/// Then we will message the async receiver who will sleep 3s then answer.
pub fn kino_local_handler(
    _message: &Message,
    _state: &mut AppState,
    _server: &mut HttpServer,
    request: AsyncBRequest,
) {
    kiprintln!("Receiver: Received request: {:?}", request);
    kiprintln!("Receiver: Sleeping for 3 seconds...");
    std::thread::sleep(std::time::Duration::from_secs(3));

    let response_body = match request {
        AsyncBRequest::StepA(my_string) => {
            kiprintln!("Receiver: Handling StepA");
            AsyncBResponse::StepA(my_string.len() as i32)
        }
        AsyncBRequest::StepB(i32_val) => {
            kiprintln!("Receiver: Handling StepB");
            AsyncBResponse::StepB(i32_val as u64 * 2)
        }
        AsyncBRequest::StepC(u64_val) => {
            kiprintln!("Receiver: Handling StepC");
            AsyncBResponse::StepC(format!("Hello from the other side C: {}", u64_val))
        }
        AsyncBRequest::Gather(_) => {
            AsyncBResponse::Gather(Ok("Hello from B".to_string()))
        },
    };

    kiprintln!("Receiver: Sending response: {:?}", response_body);
    let _ = Response::new().body(response_body).send();
}
