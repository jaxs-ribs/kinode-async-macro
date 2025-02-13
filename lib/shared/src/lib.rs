use kinode_process_lib::Address;
use process_macros::SerdeJsonInto;
use serde::{Deserialize, Serialize};

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


#[derive(Debug, Serialize, Deserialize, SerdeJsonInto, Clone)]
pub struct SomeStruct {
    pub counter: u64,
}

#[derive(Debug, Serialize, Deserialize, SerdeJsonInto, Clone)]
pub struct SomeOtherStruct {
    pub message: String,
}
