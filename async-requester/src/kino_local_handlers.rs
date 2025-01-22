use crate::*;

use kinode_app_common::send;
use kinode_process_lib::Request;
/// This will get triggered with a terminal request 
/// For example, if you run `m our@async-requester:async-app:template.os '"abc"'`
/// Then we will message the async receiver who will sleep 3s then answer.
pub fn kino_local_handler(
    _message: &Message,
    _state: &mut AppState,
    _server: &mut HttpServer,
    _request: String,
) {
    kiprintln!("Sending message to receiver");
    // send!(
    //     receiver_address(),
    //     AsyncRequest::StepA("Mashed Potatoes".to_string()),
    //     (resp, st: AppState) {
    //         kiprintln!("LALALALA");
    //     }
    // );
    // Request::to(receiver_address())
    //     .body(AsyncRequest::StepA("Mashed Potatoes".to_string()))
    //     .send();
    send_async!(
        receiver_address(),
        AsyncRequest::StepA("Mashed Potatoes".to_string()),
        (resp, st: AppState) {
            on_step_a(resp, st);
        },
    );

    /*
    Note, you can also send a timeout, and an on_timeout handler.
    
    send_async!(
        receiver_address(),
        AsyncRequest::StepB("Yes hello".to_string()),
        (resp, st: AppState) {
            custom_handler(resp, st);
        },
        30,
        on_timeout => {
            println!("timed out!");
            st.counter -= 1;
        }
    );
     */
}

fn on_step_a(response: i32, state: &mut AppState) {
    kiprintln!("{}", response);
    kiprintln!("{}", state.counter);
    state.counter += 1;
    send_async!(
        receiver_address(),
        AsyncRequest::StepB(response),
        (resp, st: AppState) {
            on_step_b(resp, st);
        },
    );
}

fn on_step_b(response: u64, state: &mut AppState) {
    kiprintln!("{}", response);
    kiprintln!("{}", state.counter);
    state.counter += 1;
    send_async!(
        receiver_address(),
        AsyncRequest::StepC(response),
        (resp, st: AppState) {
            on_step_c(resp, st);
        },
    );
}

fn on_step_c(response: String, state: &mut AppState) {
    kiprintln!("{}", response);
    kiprintln!("{}", state.counter);
    state.counter += 1;
}