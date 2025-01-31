use kinode_app_common::*;
use kinode_process_lib::Address;
use process_macros::SerdeJsonInto;
use serde::{Deserialize, Serialize};

pub fn receiver_address_a() -> Address {
    ("our", "async-receiver-a", "async-app", "uncentered.os").into()
}

pub fn requester_address() -> Address {
    ("our", "async-requester", "async-app", "uncentered.os").into()
}

declare_types! {
    Async {
        StepA String => i32
        StepB i32 => u64
        StepC u64 => String
    },
    Commodore {
        Power SomeStruct => SomeOtherStruct
        Excitement i32 => Result<String, String>
    },
}

#[derive(Debug, Serialize, Deserialize, SerdeJsonInto, Clone)]
pub struct SomeStruct {
    pub counter: u64,
}

#[derive(Debug, Serialize, Deserialize, SerdeJsonInto, Clone)]
pub struct SomeOtherStruct {
    pub message: String,
}
