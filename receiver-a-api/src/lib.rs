use crate::exports::hyperware::process::receiver_a::Guest;
use crate::exports::hyperware::process::receiver_a::*;
use crate::hyperware::process::standard::Address as WitAddress;

wit_bindgen::generate!({
    path: "target/wit",
    world: "receiver-a-api-v0",
    generate_unused_types: true,
    additional_derives: [serde::Deserialize, serde::Serialize, process_macros::SerdeJsonInto],
});

struct Api;
impl Guest for Api {
    fn call_me(target: WitAddress, value: i32) -> Result<String, String> {
                // local
        Ok("Success".to_string())
    }
}
export!(Api);
