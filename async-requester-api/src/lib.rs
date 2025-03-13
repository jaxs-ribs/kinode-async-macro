use crate::exports::hyperware::process::async_requester::Guest;
use crate::exports::hyperware::process::async_requester::*;
use crate::hyperware::process::standard::Address as WitAddress;

wit_bindgen::generate!({
    path: "target/wit",
    world: "async-requester-api-v0",
    generate_unused_types: true,
    additional_derives: [serde::Deserialize, serde::Serialize, process_macros::SerdeJsonInto],
});

struct Api;
impl Guest for Api {
    fn increment_counter(target: WitAddress, value: i32, another_value: String, yet_another_value: f32) -> Result<String, String> {
                // local
        Ok("Success".to_string())
    }
    fn increment_counter_two(target: WitAddress, value: f64, another_value: Vec<String>, yet_another_value: bool) -> Result<Vec<i32>, String> {
                // remote, local
        Ok(Vec::new())
    }
    fn increment_counter_async(target: WitAddress, value: i32, name: String) -> Result<String, String> {
                // local
        Ok("Success".to_string())
    }
    fn some_other_function(target: WitAddress, string_val: String, another_string_val: String) -> Result<f32, String> {
                // remote
        Ok(0.0)
    }
    fn increment_counter_three(target: WitAddress, string_val: String) -> Result<f32, String> {
                // local, http
        Ok(0.0)
    }
    fn increment_counter_four(target: WitAddress, string_val: String) -> Result<f32, String> {
                // local, http
        Ok(0.0)
    }
}
export!(Api);
