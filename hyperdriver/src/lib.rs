#![allow(warnings)] 

use caller_utils::receiver_b::hello_local_rpc;
// use caller_utils::receiver_b::hello_local_rpc;
// TODO: Zena: Remove this and fix warnings
use hyperprocess_macro::hyperprocess;
use hyperware::process::standard::ProcessId;
use hyperware_app_common::State;
use hyperware_process_lib::{Address, LazyLoadBlob, Request as HyperwareRequest};
use hyperware_process_lib::{http::server::WsMessageType};
use hyperware_app_common::send;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use hyperware_process_lib::kiprintln;
use hyperware::process::standard::Address as WitAddress;
use shared::{receiver_address_a, receiver_address_b};
use caller_utils::wit_custom::SomeEnum;
use caller_utils::SomeStruct;

#[derive(Default, Debug, Serialize, Deserialize)]
struct AsyncRequesterState {
    request_count: u64,
}

#[hyperprocess(
    name = "Async Requester",
    ui = Some(HttpBindingConfig::default()),
    endpoints = vec![
        Binding::Http {
            path: "/api",
            config: HttpBindingConfig::new(false, false, false, None),
        }, 
        Binding::Ws {
            path: "/ws",
            config: WsBindingConfig::new(false, false, false),
        }
    ],
    save_config = SaveOptions::EveryMessage,
    wit_world = "async-app-template-dot-os-v0"
)]
impl AsyncRequesterState {
    #[init]
    async fn initialize(&mut self) {
        kiprintln!("Initializing Async Requester");
        std::thread::sleep(std::time::Duration::from_secs(3));
        kiprintln!("Sending request");

        let result = hello_local_rpc(&receiver_address_b(), SomeStruct {
            field_one: "test".to_string(),
            field_two: 42,
            field_three: SomeEnum::VariantOne("test".to_string()),
        }).await;
        kiprintln!("Received result {:?}", result);
        // let address: Address = ("our", "receiver-a", "async-app", "uncentered.os").into();
        // let result = send::<String>(&json!({"CallMe": (42, 1337)}), address, 30).await;
        // kiprintln!("Received result {:?}", result);
        // let address: Address = ("our", "receiver-b", "async-app", "uncentered.os").into();
        // let result = send::<Value>(&json!({"Hello": "Mash Potatoes"}), address, 30).await;
        // kiprintln!("Received result {:?}", result);

        // kiprintln!("Sleeping more");
        // std::thread::sleep(std::time::Duration::from_secs(3));
        // fetch_data("users", 1337).await;
    }

    #[http]
    async fn jaxs_ribs(&mut self, value: i32) -> String {
        kiprintln!("Sending request");
        self.request_count += 1;
        kiprintln!("Counter: {}", self.request_count);
        "some string".to_string()
    }

    #[local]
    async fn increment_counter(
        &mut self,
        value: i32,
        another_value: String,
        yet_another_value: f32,
    ) -> String {
        kiprintln!("Sending request");
        self.request_count += 1;
        kiprintln!("Counter: {}", self.request_count);

        "some string".to_string()
    }

    #[remote]
    #[local]
    fn increment_counter_two(
        &mut self,
        value: f64,
        another_value: Vec<String>,
        yet_another_value: bool,
    ) -> Vec<i32> {
        self.request_count += 1;
        kiprintln!(
            "Called with: {} {:?} {}",
            value,
            another_value,
            yet_another_value
        );
        kiprintln!("Counter: {}", self.request_count);
        vec![42, 43, 44]
    }

    #[local]
    async fn increment_counter_async(&mut self, value: i32, name: String) -> String {
        self.request_count += 1;
        kiprintln!("Starting async operations for {}", name);
        let user_data = fetch_data("users", value).await;
        let stats_data = fetch_data("stats", value).await;
        let result = format!("{} | {}", user_data, stats_data);
        kiprintln!("Completed. Result: {}", result);
        format!("Results for {}: {}", name, result)
    }

    #[remote]
    fn some_other_function(&mut self, string_val: String, another_string_val: String) -> f32 {
        self.request_count += 1;
        kiprintln!(
            "We have been called with thes following values: {:?}, {:?}",
            string_val,
            another_string_val
        );
        0.0
    }

    #[local]
    #[http]
    async fn increment_counter_three(&mut self, string_val: String) -> f32 {
        self.request_count += 1;
        kiprintln!(
            "We have been called with thes following values: {:?}",
            string_val
        );
        0.0
    }

    #[local]
    #[http]
    async fn increment_counter_four(&mut self, string_val: String) -> f32 {
        self.request_count += 1;
        kiprintln!(
            "We have been called with thes following values: {:?}",
            string_val
        );
        0.0
    }

    #[ws]
    fn websocket(&mut self, channel_id: u32, message_type: WsMessageType, blob: LazyLoadBlob) {
        kiprintln!("Websocket called with: {:?}, {:?}, {:?}", channel_id, message_type, blob);
        self.request_count += 1;
        kiprintln!("Counter: {}", self.request_count);
    }
}

async fn fetch_data(endpoint: &str, id: i32) -> String {
    kiprintln!("Fetching data from {} with id {}", endpoint, id);
    // use crate::hyperware::process::receiver_b::{SomeStruct as Poob, SomeEnum};
    // use crate::hyperware_async::hello_local_rpc;
    // use crate::hyperware_async::call_me_local_rpc;

    // let address_1: Address = ("our", "receiver-a", "async-app", "uncentered.os").into();
    // let address_2: Address = ("our", "receiver-a", "async-app", "uncentered.os").into();
    // let some_struct = Poob {
    //     field_one: "test".to_string(),
    //     field_two: 42,
    //     field_three: SomeEnum::VariantOne("test".to_string()),
    // };
    // let a = hello_local_rpc(address_1, some_struct).await;
    // kiprintln!("A: {:?}", a);

    // let b = call_me_local_rpc(address_2, 32).await;
    // kiprintln!("B: {:?}", b);

    

    format!("Data from {} for id {}", endpoint, id)
}

/*
We want to be able to handle an arbitrary number of parameters for a request.
m our@hyperdriver:async-app:uncentered.os '{"IncrementCounter": [42, "abc", 3.14]}'
m our@hyperdriver:async-app:uncentered.os '{"IncrementCounter2": [42.0, ["abc", "def"], true]}'
m our@hyperdriver:async-app:uncentered.os '{"IncrementCounterAsync": [42, "test-user"]}'

curl -X POST -H "Content-Type: application/json" -d '{"IncrementCounter3": "test-string"}' http://localhost:8080/hyperdriver:async-app:uncentered.os/api
curl -X POST -H "Content-Type: application/json" -d '{"IncrementCounter4": "test-string"}' http://localhost:8080/hyperdriver:async-app:uncentered.os/api
*/
