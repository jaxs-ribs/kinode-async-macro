use crate::*;
use kinode_process_lib::timer::TimerAction;

/// This will get triggered with a terminal request
/// For example, if you run `m our@async-requester:async-app:template.os '"abc"'`
/// Then we will message the async receiver who will sleep 3s then answer.
pub fn repeated_timer(state: &mut ProcessState) {
    kiprintln!("Repeated timer called! Our counter is {}", state.counter);
    timer!(3000, (st: ProcessState) {
        st.counter += 1;
        repeated_timer(st);
    });
}

pub fn kino_local_handler(
    _message: &Message,
    _state: &mut ProcessState,
    _server: &mut HttpServer,
    _request: String,
) {
    // In this case, this gets called on terminal command (usually)
    message_a();
}

fn message_a() {
    send_async!(
        receiver_address_a(),
        AsyncARequest::StepA("Mashed Potatoes".to_string()),
        (resp, st: ProcessState) {
            on_step_a(resp, st);
        },
    );
}

fn on_step_a(response: i32, state: &mut ProcessState) {
    kiprintln!("Sender: Received response: {}", response);
    kiprintln!("Sender: State: {}", state.counter);
    state.counter += 1;
    send_async!(
        receiver_address_a(),
        AsyncARequest::StepB(response),
        (resp, st: ProcessState) {
            let _ = on_step_b(resp, st);
        },
    );
}

fn on_step_b(response: u64, state: &mut ProcessState) -> anyhow::Result<()> {
    kiprintln!("Sender: Received response: {}", response);
    kiprintln!("Sender: State: {}", state.counter);
    state.counter += 1;
    send_async!(
        receiver_address_a(),
        AsyncARequest::StepC(response),
        (resp, st: ProcessState) {
            on_step_c(resp, st);
        },
    );
    Ok(())
}

fn on_step_c(response: String, state: &mut ProcessState) {
    kiprintln!("Sender: Received response: {}", response);
    kiprintln!("Sender: State: {}", state.counter);
    state.counter += 1;
}
