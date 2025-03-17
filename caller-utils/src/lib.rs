use wit_parser::wit_parser;
wit_parser!("api/async-app-template-dot-os-v0.wit");

/// Generated caller utilities for RPC function stubs

pub use hyperware_app_common::SendResult;
pub use hyperware_app_common::send;
use hyperware_process_lib::Address;
use serde_json::json;

// Import specific types from each interface
pub use crate::wit_custom::SomeStruct;
pub use crate::wit_custom::SomeEnum;

/// Generated RPC stubs for the receiver_b interface
pub mod receiver_b {
    use crate::*;

    /// Generated stub for `hello` local RPC call
    pub async fn hello_local_rpc(target: &Address, struct_val: SomeStruct) -> SendResult<f32> {
        let request = json!({"Hello": struct_val});
        send::<f32>(&request, target, 30).await
    }
    
    
}

/// Generated RPC stubs for the async_requester interface
pub mod async_requester {
    use crate::*;

    /// Generated stub for `jaxs-ribs` http RPC call
    pub async fn jaxs_ribs_http_rpc(_target: &str, _value:  i32) -> SendResult<String> {
        // TODO: Implement HTTP endpoint
        SendResult::Success(String::new())
    }
    
    /// Generated stub for `increment-counter` local RPC call
    pub async fn increment_counter_local_rpc(target: &Address, value: i32, another_value: String, yet_another_value: f32) -> SendResult<String> {
        let request = json!({"IncrementCounter": (value, another_value, yet_another_value)});
        send::<String>(&request, target, 30).await
    }
    
    /// Generated stub for `increment-counter-two` remote RPC call
    pub async fn increment_counter_two_remote_rpc(target: &Address, value: f64, another_value: Vec<String>, yet_another_value: bool) -> SendResult<Vec<i32>> {
        let request = json!({"IncrementCounterTwo": (value, another_value, yet_another_value)});
        send::<Vec<i32>>(&request, target, 30).await
    }
    
    /// Generated stub for `increment-counter-two` local RPC call
    pub async fn increment_counter_two_local_rpc(target: &Address, value: f64, another_value: Vec<String>, yet_another_value: bool) -> SendResult<Vec<i32>> {
        let request = json!({"IncrementCounterTwo": (value, another_value, yet_another_value)});
        send::<Vec<i32>>(&request, target, 30).await
    }
    
    /// Generated stub for `increment-counter-async` local RPC call
    pub async fn increment_counter_async_local_rpc(target: &Address, value: i32, name: String) -> SendResult<String> {
        let request = json!({"IncrementCounterAsync": (value, name)});
        send::<String>(&request, target, 30).await
    }
    
    /// Generated stub for `some-other-function` remote RPC call
    pub async fn some_other_function_remote_rpc(target: &Address, string_val: String, another_string_val: String) -> SendResult<f32> {
        let request = json!({"SomeOtherFunction": (string_val, another_string_val)});
        send::<f32>(&request, target, 30).await
    }
    
    /// Generated stub for `increment-counter-three` local RPC call
    pub async fn increment_counter_three_local_rpc(target: &Address, string_val: String) -> SendResult<f32> {
        let request = json!({"IncrementCounterThree": string_val});
        send::<f32>(&request, target, 30).await
    }
    
    /// Generated stub for `increment-counter-three` http RPC call
    pub async fn increment_counter_three_http_rpc(_target: &str, _string_val:  String) -> SendResult<f32> {
        // TODO: Implement HTTP endpoint
        SendResult::Success(0.0)
    }
    
    /// Generated stub for `increment-counter-four` local RPC call
    pub async fn increment_counter_four_local_rpc(target: &Address, string_val: String) -> SendResult<f32> {
        let request = json!({"IncrementCounterFour": string_val});
        send::<f32>(&request, target, 30).await
    }
    
    /// Generated stub for `increment-counter-four` http RPC call
    pub async fn increment_counter_four_http_rpc(_target: &str, _string_val:  String) -> SendResult<f32> {
        // TODO: Implement HTTP endpoint
        SendResult::Success(0.0)
    }
    
    
}

/// Generated RPC stubs for the receiver_a interface
pub mod receiver_a {
    use crate::*;

    /// Generated stub for `call-me` local RPC call
    pub async fn call_me_local_rpc(target: &Address, value: i32, another_value: i32) -> SendResult<String> {
        let request = json!({"CallMe": (value, another_value)});
        send::<String>(&request, target, 30).await
    }
    
    
}

