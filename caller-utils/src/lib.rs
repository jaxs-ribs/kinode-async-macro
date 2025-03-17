
use wit_parser::wit_parser;
wit_parser!("api/async-app-template-dot-os-v0.wit");

pub use crate::wit_custom::SomeStruct;

use serde_json::json;
use hyperware_app_common::send;
use hyperware_app_common::SendResult;
use hyperware_process_lib::Address;

pub mod receiver_b {
    use crate::*;

    /// Generated stub for `hello` local RPC call
    /// This function provides a placeholder implementation that returns default values
    pub async fn hello_local_rpc(target: &Address, struct_val: SomeStruct) -> SendResult<f32> {
        let request = json!({"Hello": struct_val});
        send::<f32>(&request, target, 30).await
    }
}

// /// Generated RPC stubs for the receiver_a interface
// pub mod receiver_a {
//     use crate::*;

//     /// Generated stub for `call-me` local RPC call
//     /// This function provides a placeholder implementation that returns default values
//     pub async fn call_me_local_rpc(target: &WitAddress, value: i32, another_value: i32) -> String {
//         // TODO: Implement actual RPC call
//         String::new()
//     }
    
    
// }

// /// Generated RPC stubs for the async_requester interface
// pub mod async_requester {
//     use crate::*;

//     /// Generated stub for `jaxs-ribs` http RPC call
//     /// This function provides a placeholder implementation that returns default values
//     pub async fn jaxs_ribs_http_rpc(target: &str, value: i32) -> String {
//         // TODO: Implement actual RPC call
//         String::new()
//     }
    
//     /// Generated stub for `increment-counter` local RPC call
//     /// This function provides a placeholder implementation that returns default values
//     pub async fn increment_counter_local_rpc(target: &WitAddress, value: i32, another_value: String, yet_another_value: f32) -> String {
//         // TODO: Implement actual RPC call
//         String::new()
//     }
    
//     /// Generated stub for `increment-counter-two` remote RPC call
//     /// This function provides a placeholder implementation that returns default values
//     pub async fn increment_counter_two_remote_rpc(target: &WitAddress, value: f64, another_value: Vec<String>, yet_another_value: bool) -> Vec<i32> {
//         // TODO: Implement actual RPC call
//         Vec::new()
//     }
    
//     /// Generated stub for `increment-counter-two` local RPC call
//     /// This function provides a placeholder implementation that returns default values
//     pub async fn increment_counter_two_local_rpc(target: &WitAddress, value: f64, another_value: Vec<String>, yet_another_value: bool) -> Vec<i32> {
//         // TODO: Implement actual RPC call
//         Vec::new()
//     }
    
//     /// Generated stub for `increment-counter-async` local RPC call
//     /// This function provides a placeholder implementation that returns default values
//     pub async fn increment_counter_async_local_rpc(target: &WitAddress, value: i32, name: String) -> String {
//         // TODO: Implement actual RPC call
//         String::new()
//     }
    
//     /// Generated stub for `some-other-function` remote RPC call
//     /// This function provides a placeholder implementation that returns default values
//     pub async fn some_other_function_remote_rpc(target: &WitAddress, string_val: String, another_string_val: String) -> f32 {
//         // TODO: Implement actual RPC call
//         0.0
//     }
    
//     /// Generated stub for `increment-counter-three` local RPC call
//     /// This function provides a placeholder implementation that returns default values
//     pub async fn increment_counter_three_local_rpc(target: &WitAddress, string_val: String) -> f32 {
//         // TODO: Implement actual RPC call
//         0.0
//     }
    
//     /// Generated stub for `increment-counter-three` http RPC call
//     /// This function provides a placeholder implementation that returns default values
//     pub async fn increment_counter_three_http_rpc(target: &str, string_val: String) -> f32 {
//         // TODO: Implement actual RPC call
//         0.0
//     }
    
//     /// Generated stub for `increment-counter-four` local RPC call
//     /// This function provides a placeholder implementation that returns default values
//     pub async fn increment_counter_four_local_rpc(target: &WitAddress, string_val: String) -> f32 {
//         // TODO: Implement actual RPC call
//         0.0
//     }
    
//     /// Generated stub for `increment-counter-four` http RPC call
//     /// This function provides a placeholder implementation that returns default values
//     pub async fn increment_counter_four_http_rpc(target: &str, string_val: String) -> f32 {
//         // TODO: Implement actual RPC call
//         0.0
//     }
    
    
// }

