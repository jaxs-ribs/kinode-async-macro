use crate::exports::hyperware::process::receiver_b::Guest;
use crate::exports::hyperware::process::receiver_b::*;
use crate::hyperware::process::standard::Address as WitAddress;

wit_bindgen::generate!({
    path: "target/wit",
    world: "receiver-b-api-v0",
    generate_unused_types: true,
    additional_derives: [serde::Deserialize, serde::Serialize, process_macros::SerdeJsonInto],
});

struct Api;
impl Guest for Api {
    fn hello(target: WitAddress, struct_val: SomeStruct) -> Result<f32, String> {
                // local
        Ok(0.0)
    }
}
export!(Api);
