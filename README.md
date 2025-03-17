# Cronchware

## Overview

Cronchware is a specialized RPC (Remote Procedure Call) framework for WebAssembly-based distributed systems built on Hyperware (WASM-WASI). Its core purpose is to provide type-safe, macro-generated RPC functions between WebAssembly components with completely abstracted message passing.

## Key Workflow

The workflow is specifically designed to generate interface types from code (opposite to traditional bindgen approaches):

1. **Write Rust Code**
   - Define state structs with `#[hyperprocess]` attribute
   - Add methods with attributes (`#[local]`, `#[remote]`, `#[http]`, etc.)

2. **Generate WIT Files**
   - Run the `bababooey` command
   - This analyzes your Rust code and automatically generates WIT interface files
   - Creates interface definitions with proper types in kebab-case

3. **Build The Project**
   - Execute `kit b` (customized cargo build)
   - Uses the generated WIT files to create bindings
   - Places WIT files in appropriate target paths

4. **Use Generated Async Functions**
   - Import the generated functions from `hyperware_async` module
   - Make RPC calls with type-safe signatures and `async/await` syntax

This workflow (Rust → WIT → Bindings) is the reverse of traditional bindgen approaches, enabling a developer-friendly experience where interface definitions are automatically derived from implementation.

## Architecture

The system consists of these core components:

### 1. Runtime Foundation (`hyperware_app_common.rs`)

A single-threaded process runtime that:
- Provides a lightweight async executor
- Manages the message loop with `await_message()`
- Handles serialization/deserialization
- Manages state persistence
- Abstracts correlation IDs and response tracking

### 2. Process Definition (`hyperprocess_macro.rs`)

The `#[hyperprocess]` macro transforms a Rust implementation into:
- A complete WebAssembly component
- Message handling for different call types
- Automatic state management
- HTTP/WebSocket bindings when requested

### 3. WIT Generation (`paste-2.txt` - bababooey command)

Analyzes Rust code and generates:
- Interface definitions in WIT format
- Type transformations (Rust → WIT types)
- Function signature records for each call type

### 4. RPC Binding Generation (`hyper_bindgen.rs`)

Takes WIT files and generates:
- Async RPC functions for callers
- Message handling code for callees
- Type-safe stubs that abstract away all message passing

## Message Flow

```
┌──────────────────────┐                        ┌──────────────────────┐
│ Caller Process        │                        │ Callee Process       │
│                       │                        │                      │
│ 1. Call generated     │                        │                      │
│    async function     │                        │                      │
│    ────────────────►  │                        │                      │
│    increment_remote_  │  2. Serialize request  │                      │
│    rpc(addr, val)     │     with unique ID     │                      │
│                       │     ───────────────►   │                      │
│                       │                        │                      │
│                       │  3. Send message via   │                      │
│                       │     Hyperware          │  4. Receive message  │
│                       │     ───────────────────►  ──────────────►     │
│                       │                        │                      │
│                       │                        │  5. Deserialize and  │
│                       │                        │     dispatch to      │
│                       │                        │     appropriate      │
│                       │                        │     handler          │
│                       │                        │                      │
│                       │                        │  6. Execute handler  │
│                       │                        │     method           │
│                       │                        │                      │
│                       │                        │  7. Serialize result │
│                       │                        │     ──────────────►  │
│  10. Resolve future   │  9. Receive response   │                      │
│      with result      │     ◄───────────────   │  8. Send response    │
│      ◄────────────────┤                        │     with same ID     │
│                       │                        │     ◄───────────────┐│
└──────────────────────┘                        └──────────────────────┘
        Async                                           Sync
      Execution                                       Execution
```

## WIT-Based Communication

The WIT files serve as the interface substrate between processes:

- **For Callers**: WIT defines what functions are available and their signatures
- **For Callees**: WIT defines the message format and expected responses

The system uses signature records in WIT to define function types until async WIT functions are available in WASI Preview 3:

```wit
// Example generated WIT record for function signatures
record increment-counter-signature-remote {
    target: address,
    value: s32,
    returning: string
}
```

## Example Usage

### 1. Define Process

```rust
#[hyperprocess(
    name = "Counter Process",
    endpoints = vec![...],
    save_config = SaveOptions::EveryMessage,
    wit_world = "counter-app-v0"
)]
impl CounterState {
    #[init]
    async fn initialize(&mut self) {
        self.count = 0;
    }
    
    #[remote]
    fn increment(&mut self, value: i32) -> i32 {
        self.count += value;
        self.count
    }
}
```

### 2. Generate WIT & Build

```bash
$ bababooey           # Analyze code and generate WIT files
$ kit b               # Build with generated WIT files
```

### 3. Use Generated Functions

```rust
use crate::hyperware_async::increment_remote_rpc;

async fn perform_increment() {
    let target = Address::new("node", "counter", "counting-pkg", "publisher");
    let result = increment_remote_rpc(target, 5).await;
    println!("New count: {}", result);
}
```

## Core Value Proposition

1. **Code-First Interface Generation**: Generate WIT files from code rather than manually defining them
2. **Type Safety Across Processes**: Full type checking between caller and callee
3. **Abstracted Message Passing**: Hide all serialization, messaging, and async details
4. **Simplified Development**: Write normal Rust functions, get RPC for free
5. **WASM-Optimized**: Specifically designed for Hyperware's WASM-WASI environment

## Key Implementation Details

### Message Tracking

- Each request gets a unique correlation ID (UUID)
- Responses are matched to waiting futures using this ID
- The system handles timeouts and error propagation

### State Management

Processes can control their persistence strategy:

```rust
save_config = SaveOptions::EveryMessage  // Save after every message
save_config = SaveOptions::EveryNMessage(10)  // Save every 10 messages
save_config = SaveOptions::EveryNSeconds(60)  // Save every minute
save_config = SaveOptions::Never  // Never save
```

### Current Limitations

- Uses record-based approach in WIT instead of direct function definitions (temporary until WASI Preview 3)
- Single-threaded execution model (one message at a time)
- Must define all possible call patterns with appropriate attributes

## Maintaining and Extending

### Key Files and Their Purposes:

- `hyperprocess_macro.rs` - The core macro implementation
- `hyper_bindgen.rs` - RPC function generation
- `paste-2.txt` (bababooey) - WIT generation from Rust code
- `hyperware_app_common.rs` - Runtime support and state management

When making changes, ensure alignment between:
1. How processes are defined (macro implementation)
2. How WIT files are generated (bababooey)
3. How RPC functions are generated (binding generator)
4. How messages are processed at runtime (common library)