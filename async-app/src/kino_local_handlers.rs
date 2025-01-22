use crate::*;

pub fn kino_local_handler(
    _message: &Message,
    _state: &mut AppState,
    _server: &mut HttpServer,
    _request: String,
) {
    send_async!(
        receiver_address(),
        AsyncRequest::StepA("Mashed Potatoes".to_string()),
        (resp, st: AppState) {
            custom_handler_a(resp, st);
        },
    );

    // send_async!(
    //     receiver_address(),
    //     AsyncRequest::StepB("Yes hello".to_string()),
    //     (resp, st: AppState) {
    //         custom_handler(resp, st);
    //     },
    //     30,
    //     on_timeout => {
    //         println!("timed out!");
    //         st.counter -= 1;
    //     }
    // );
}

fn custom_handler_a(response: i32, state: &mut AppState) {
    kiprintln!("{}", response);
    kiprintln!("{}", state.counter);
    state.counter += 1;
    send_async!(
        receiver_address(),
        AsyncRequest::StepB(response),
        (resp, st: AppState) {
            custom_handler_b(resp, st);
        },
    );
}

fn custom_handler_b(response: u64, state: &mut AppState) {
    kiprintln!("{}", response);
    kiprintln!("{}", state.counter);
    state.counter += 1;
    send_async!(
        receiver_address(),
        AsyncRequest::StepC(response),
        (resp, st: AppState) {
            custom_handler_c(resp, st);
        },
    );
}

fn custom_handler_c(response: String, state: &mut AppState) {
    kiprintln!("{}", response);
    kiprintln!("{}", state.counter);
    state.counter += 1;
}