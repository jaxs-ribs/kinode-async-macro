use hyperware_process_lib::Address;

pub fn receiver_address_a() -> Address {
    ("our", "receiver-a", "async-app", "uncentered.os").into()
}

pub fn receiver_address_b() -> Address {
    ("our", "receiver-b", "async-app", "uncentered.os").into()
}

pub fn requester_address() -> Address {
    ("our", "async-requester", "async-app", "uncentered.os").into()
}
