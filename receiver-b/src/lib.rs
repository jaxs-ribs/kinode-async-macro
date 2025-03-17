#![allow(warnings)] // TODO: Zena: Remove this and fix warnings
use hyperprocess_macro::hyperprocess;
use hyperware_process_lib::http::server::WsMessageType;
use hyperware_process_lib::logging::info;
use hyperware_process_lib::LazyLoadBlob;
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Serialize, Deserialize)]
struct ReceiverBState {
    request_count: u64,
}

#[hyperprocess(
    name = "Receiver B",
    ui = Some(HttpBindingConfig::default()),
    endpoints = vec![],
    save_config = SaveOptions::EveryMessage,
    wit_world = "async-app-template-dot-os-v0"
)]
impl ReceiverBState {
    #[init]
    async fn initialize(&mut self) {
        info!("Initializing Receiver B");
        self.request_count = 0;
        info!("The counter is now {}", self.request_count);
    }

    #[local]
    fn hello(&mut self, struct_val: SomeStruct) -> f32 {
        let num_chars = struct_val.field_one.len() as f32;
        let num_chars_enum = match struct_val.field_three {
            SomeEnum::VariantOne(s) => s.len() as f32,
            SomeEnum::VariantTwo(i) => i as f32,
        };
        info!("Received string of length {}", num_chars);
        num_chars
    }
}

#[derive(Debug, Serialize, Deserialize)]
enum SomeEnum {
    VariantOne(String),
    VariantTwo(i32),
}

#[derive(Debug, Serialize, Deserialize)]
struct SomeStruct {
    field_one: String,
    field_two: i32,
    field_three: SomeEnum,
}
