use crate::*;

/// This will get triggered with a terminal request
/// For example, if you run `m our@async-app:async-app:template.os '"abc"'`
/// Then we will message the async receiver who will sleep 3s then answer.
pub fn kino_local_handler(
    message: &Message,
    _state: &mut AppState,
    _server: &mut HttpServer,
    _request: String,
) {
    /*
    TODO: Zena:
    There are a few assumptions we can generally make:
    - We will never receive responses in this framework. We only send requests and handle with callbacks.
    - Do we actually have to deserialize the body manually? What if we just receive the body?
     */
    kiprintln!("Receiver received a message");
    let Message::Request { body, .. } = message else {
        kiprintln!("I don't think we should ever get there in this framework. We only handle requests now. ");
        return;
    };
    let Ok(async_request) = serde_json::from_slice::<AsyncRequest>(body) else {
        kiprintln!("Failed to deserialize body");
        return;
    };

    kiprintln!("Sleeping for 3 seconds...");
    std::thread::sleep(std::time::Duration::from_secs(3));

    let response_body = match async_request {
        AsyncRequest::StepA(my_string) => {
            kiprintln!("Handling StepA");
            AsyncResponse::StepA(my_string.len() as i32)
        }
        AsyncRequest::StepB(i32_val) => {
            kiprintln!("Handling StepB");
            AsyncResponse::StepB(i32_val as u64 * 2)
        }
        AsyncRequest::StepC(u64_val) => {
            kiprintln!("Handling StepC");
            AsyncResponse::StepC(format!("Hello from the other side C: {}", u64_val))
        }
    };

    kiprintln!("Sending response: {:?}", response_body);
    let _ = Response::new().body(response_body).send();
}
