use hyperware_process_lib::Address;

pub fn receiver_address_a() -> Address {
    ("our", "async-receiver-a", "async-app", "uncentered.os").into()
}

pub fn receiver_address_b() -> Address {
    ("our", "async-receiver-b", "async-app", "uncentered.os").into()
}

pub fn receiver_address_c() -> Address {
    ("our", "async-receiver-c", "async-app", "uncentered.os").into()
}

pub fn requester_address() -> Address {
    ("our", "async-requester", "async-app", "uncentered.os").into()
}
