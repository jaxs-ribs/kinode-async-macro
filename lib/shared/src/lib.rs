use kinode_app_common::*;
use process_macros::SerdeJsonInto;
use serde::{Deserialize, Serialize};
use kinode_process_lib::Address;

pub fn receiver_address() -> Address {
    ("our", "async-receiver", "async-callbacks", "template.os").into()
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
