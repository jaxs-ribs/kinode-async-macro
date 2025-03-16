// this is hyper_bindgen
extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{quote, format_ident};
use syn::{LitStr, Ident, Token, parse::Parse, parse::ParseStream, Result, Error}; // Removed parse_macro_input
use std::fs;
use std::path::PathBuf;
use proc_macro2::Span;

/// A procedural macro that generates async Rust functions based on WIT interface definitions.
#[proc_macro]
pub fn generate(input: TokenStream) -> TokenStream {
    // Check if we're running inside Rust Analyzer - if so, generate simple stubs
    // This greatly improves Rust Analyzer stability
    if std::env::var("RUST_ANALYZER").is_ok() || std::env::var("__RA_ANALYZER_ACTIVE").is_ok() {
        return generate_rust_analyzer_stubs(input);
    }
    
    // For actual compilation, use the full implementation
    hyper_bindgen(input)
}

// Generate simplified stubs for Rust Analyzer to avoid crashes
fn generate_rust_analyzer_stubs(input: TokenStream) -> TokenStream {
    // Parse just enough to extract the world name
    let input_str = input.to_string();
    let _world_name = if let Some(start) = input_str.find("world:") { // Added underscore prefix
        if let Some(end) = input_str[start..].find(',') {
            &input_str[start + 7..start + end - 1]
        } else {
            "unknown_world"
        }
    } else {
        "unknown_world"
    };
    
    // Generate simple stub module for Rust Analyzer
    let output = quote! {
        /// This module contains generated async RPC functions based on WIT signature records.
        /// Note: This is a simplified version for Rust Analyzer - actual functions will be generated during compilation.
        pub mod hyperware_async {
            /// Example async function (stub for Rust Analyzer).
            /// 
            /// In an actual build, all functions from the WIT definitions will be generated here.
            pub async fn example_function(target: hyperware_process_lib::Address) -> String {
                String::from("Stub function for Rust Analyzer")
            }
            
            /// Another example async function (stub for Rust Analyzer).
            pub async fn example_http_function(target: String) -> i32 {
                42 // Stub return value
            }
        }
    };
    
    output.into()
}

// Input parser for hyper_bindgen macro - similar to wit_bindgen format
struct HyperBindgenInput {
    path: Option<String>,
    world: String,
}

impl Parse for HyperBindgenInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        syn::braced!(content in input);
        
        let mut path = None;
        let mut world = None;
        
        // Parse key-value pairs like path: "target/wit"
        while !content.is_empty() {
            let key: Ident = content.parse()?;
            content.parse::<Token![:]>()?;
            
            match key.to_string().as_str() {
                "path" => {
                    let value: LitStr = content.parse()?;
                    path = Some(value.value());
                },
                "world" => {
                    let value: LitStr = content.parse()?;
                    world = Some(value.value());
                },
                _ => {
                    // Skip other parameters (we'll ignore them)
                    content.parse::<proc_macro2::TokenTree>()?;
                }
            }
            
            // Skip trailing comma
            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            }
        }
        
        let world = world.ok_or_else(|| Error::new(Span::call_site(), "Missing 'world' parameter"))?;
        
        Ok(HyperBindgenInput {
            path,
            world,
        })
    }
}

// Function signature extracted from WIT signature record
struct FunctionSignature {
    name: String,         // Function name (snake_case)
    original_name: String, // Original kebab-case function name
    call_type: String,    // local, remote, http
    is_http: bool,        // Whether this is an HTTP call type
    params: Vec<(String, String)>, // (name, rust_type)
    return_type: String,  // Rust return type
}

// Main implementation of the hyper_bindgen procedural macro
fn hyper_bindgen(input: TokenStream) -> TokenStream {
    // Parse the macro input - use error handling to avoid crashes
    let input = match syn::parse::<HyperBindgenInput>(input) {
        Ok(input) => input,
        Err(e) => return e.to_compile_error().into(), // Added .into() here
    };
    
    let path_hint = input.path;
    let world_name = &input.world;
    
    println!("hyper_bindgen: Starting function generation for world '{}'", world_name);
    
    // Try to find and parse the world file and its imports
    let signatures = match find_and_parse_signatures(&path_hint, world_name) {
        Ok(sigs) => sigs,
        Err(e) => return e.to_compile_error().into(), // Added .into() here
    };
    
    // Print information about the functions being generated
    print_generated_functions_info(&signatures);
    
    // Generate the async functions
    let output = generate_async_functions(&signatures);
    
    output.into()
}

// Find and parse WIT files to extract function signatures
fn find_and_parse_signatures(path_hint: &Option<String>, world_name: &str) -> Result<Vec<FunctionSignature>> {
    // Check multiple possible base directories for WIT files
    let mut base_dirs = vec![
        PathBuf::from("."),                    // Current directory
        PathBuf::from("target").join("wit"),   // target/wit
        PathBuf::from("target"),               // target directory
        PathBuf::from("api"),                  // api directory
        PathBuf::from("wit"),                  // wit subdirectory
        PathBuf::new(),                        // Relative paths only
    ];
    
    // If a path hint was provided, add it first
    if let Some(path) = path_hint {
        base_dirs.insert(0, PathBuf::from(path));
    }
    
    // Generate paths to check for world file
    let possible_world_paths: Vec<_> = base_dirs.iter().flat_map(|base_dir| {
        vec![
            base_dir.join(format!("{}.wit", world_name)),
            base_dir.join(world_name).with_extension("wit"),
        ]
    }).collect();
    
    // Try to find the world file
    let world_file = possible_world_paths.iter()
        .find(|path| path.exists())
        .ok_or_else(|| {
            Error::new(
                Span::call_site(),
                format!(
                    "Could not find world file '{}'. Checked paths:\n{}", 
                    world_name, 
                    possible_world_paths.iter()
                        .map(|p| format!("  - {}", p.display()))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            )
        })?;
    
    // Read the world file to find imports
    let world_content = fs::read_to_string(world_file)
        .map_err(|e| Error::new(
            Span::call_site(),
            format!("Failed to read world file: {} - {}", world_file.display(), e)
        ))?;
    
    // Extract imports from the world file
    let imports = extract_imports(&world_content);
    
    // Parse all imported interface files to find function signatures
    let mut all_signatures = Vec::new();
    
    // Get the directory containing the world file to search for interface files
    let mut interface_base_dirs = base_dirs.clone();
    if let Some(parent) = world_file.parent() {
        let parent_path = parent.to_path_buf();
        if !interface_base_dirs.iter().any(|p| p == &parent_path) {
            interface_base_dirs.insert(0, parent_path);
        }
    }
    
    // For each import, try to find and parse the interface file
    for import in imports {
        // Try different possible locations for the interface file
        let possible_interface_paths: Vec<_> = interface_base_dirs.iter().flat_map(|base_dir| {
            vec![
                base_dir.join(format!("{}.wit", &import)),
                base_dir.join(&import).with_extension("wit"),
            ]
        }).collect();
        
        let interface_file = possible_interface_paths.iter()
            .find(|path| path.exists())
            .ok_or_else(|| {
                Error::new(
                    Span::call_site(),
                    format!(
                        "Could not find interface file for '{}'. Tried paths:\n{}", 
                        &import, 
                        possible_interface_paths.iter()
                            .map(|p| format!("  - {}", p.display()))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                )
            })?;
        
        // Read and parse the interface file
        let content = fs::read_to_string(interface_file)
            .map_err(|e| Error::new(
                Span::call_site(),
                format!("Failed to read interface file: {} - {}", interface_file.display(), e)
            ))?;
        
        let signatures = extract_signatures_from_interface(&content, &import);
        all_signatures.extend(signatures);
    }
    
    Ok(all_signatures)
}

// Extract imports from a world WIT file
fn extract_imports(content: &str) -> Vec<String> {
    let mut imports = Vec::new();
    
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("import ") {
            let import = line.trim_start_matches("import ")
                .trim_end_matches(';')
                .trim();
            imports.push(import.to_string());
        }
    }
    
    imports
}

// Extract function signatures from an interface WIT file
fn extract_signatures_from_interface(content: &str, interface_name: &str) -> Vec<FunctionSignature> {
    let mut signatures = Vec::new();
    
    // Track the current comment type before a function declaration
    let mut current_call_type: Option<String> = None;
    
    // First try to find modern function declarations:
    // function-name: func(params) -> result<return_type, error_type>;
    let mut current_record = None;
    let mut in_record = false;
    
    // Process line by line
    for line in content.lines() {
        let trimmed_line = line.trim();
        
        // Look for record declarations with the pattern:
        // record {function-name}-signature-{type} {
        if trimmed_line.starts_with("record ") && trimmed_line.contains("-signature-") {
            // Extract record name - be more flexible with the format
            let mut record_name = trimmed_line.trim_start_matches("record ").trim().to_string();
            
            // Remove trailing "{" if present
            if record_name.ends_with(" {") {
                record_name = record_name.trim_end_matches(" {").to_string();
            } else if record_name.ends_with("{") {
                record_name = record_name.trim_end_matches("{").to_string();
            }
            
            // Parse the record name to extract function name and call type
            if let Some((fn_name, call_type)) = parse_signature_record_name(&record_name) {
                // Found a record-based signature
                
                // Determine if this is an HTTP call type
                let is_http = call_type == "http";
                
                current_record = Some(FunctionSignature {
                    name: kebab_to_snake(&fn_name),
                    original_name: fn_name,
                    call_type: call_type.to_string(),
                    is_http,
                    params: Vec::new(),
                    return_type: "()".to_string(), // Default
                });
                
                in_record = true;
            }
        }
        else if in_record && trimmed_line == "}" {
            // End of record
            if let Some(record) = current_record.take() {
                signatures.push(record);
            }
            in_record = false;
        }
        else if in_record && trimmed_line.contains(":") {
            // Record field
            let parts: Vec<&str> = trimmed_line.split(':').collect();
            if parts.len() == 2 {
                let field_name = parts[0].trim().to_string();
                let mut field_type = parts[1].trim().to_string();
                
                // Remove trailing comma if present
                if field_type.ends_with(',') {
                    field_type = field_type[0..field_type.len()-1].to_string();
                }
                
                if let Some(ref mut record) = current_record {
                    if field_name == "returning" {
                        // Set return type
                        record.return_type = wit_type_to_rust(&field_type, interface_name);
                    }
                    else if field_name != "target" {  // Skip target, handled separately
                        // Add parameter
                        let param_name = kebab_to_snake(&field_name);
                        let param_type = wit_type_to_rust(&field_type, interface_name);
                        record.params.push((param_name, param_type));
                    }
                }
            }
        }
        
        // Check for call type comments
        if trimmed_line.starts_with("//") {
            let comment = trimmed_line.trim_start_matches("//").trim();
            if ["local", "remote", "http"].contains(&comment) {
                current_call_type = Some(comment.to_string());
            }
        }
        
        // Look for modern function definitions with the format:
        // function-name: func(params) -> result<return_type, error_type>;
        if trimmed_line.contains(": func(") && trimmed_line.contains("->") && trimmed_line.ends_with(";") {
            // Extract function name
            if let Some(name_end) = trimmed_line.find(": func(") {
                let fn_name = trimmed_line[0..name_end].trim();
                
                // Make sure we have a call type
                let call_type = current_call_type.clone().unwrap_or_else(|| "local".to_string());
                
                // Determine if this is an HTTP call type
                let is_http = call_type == "http";
                
                // Extract parameters
                let mut params = Vec::new();
                if let Some(params_start) = trimmed_line.find("(") {
                    if let Some(params_end) = trimmed_line.find(")") {
                        let params_str = &trimmed_line[params_start + 1..params_end];
                        
                        // Split parameters by comma
                        for param in params_str.split(',') {
                            let param = param.trim();
                            if param.is_empty() {
                                continue;
                            }
                            
                            // Parse parameter format: "name: type"
                            if let Some(colon_pos) = param.find(':') {
                                let param_name = param[0..colon_pos].trim();
                                let param_type = param[colon_pos + 1..].trim();
                                
                                // Skip target parameter
                                if param_name == "target" {
                                    continue;
                                }
                                
                                params.push((
                                    kebab_to_snake(param_name),
                                    wit_type_to_rust(param_type, interface_name)
                                ));
                            }
                        }
                    }
                }
                
                // Extract return type
                let mut return_type = "()".to_string();
                if let Some(return_start) = trimmed_line.find("->") {
                    let return_part = trimmed_line[return_start + 2..].trim();
                    
                    // Handle result<T, E> format
                    if return_part.starts_with("result<") && return_part.contains(",") {
                        // Extract successful result type (first generic param)
                        let success_type_start = return_part.find('<').map(|i| i + 1).unwrap_or(0);
                        let success_type_end = return_part.find(',').unwrap_or(return_part.len());
                        
                        if success_type_start > 0 && success_type_end > success_type_start {
                            let success_type = return_part[success_type_start..success_type_end].trim();
                            return_type = wit_type_to_rust(success_type, interface_name);
                        }
                    } else {
                        // If not a result type, handle it directly
                        return_type = wit_type_to_rust(return_part.trim_end_matches(';'), interface_name);
                    }
                }
                
                // Create the function signature
                signatures.push(FunctionSignature {
                    name: kebab_to_snake(fn_name),
                    original_name: fn_name.to_string(),
                    call_type,
                    is_http,
                    params,
                    return_type,
                });
            }
        }
    }
    
    signatures
}

// Print information about the functions being generated with import examples
fn print_generated_functions_info(signatures: &[FunctionSignature]) {
    if signatures.is_empty() {
        println!("hyper_bindgen: No functions found to generate");
        return;
    }

    // Group functions by call type for better organization
    let mut by_call_type: std::collections::HashMap<String, Vec<&FunctionSignature>> = std::collections::HashMap::new();
    
    for sig in signatures {
        by_call_type
            .entry(sig.call_type.clone())
            .or_default()
            .push(sig);
    }
    
    println!("hyper_bindgen: ===== GENERATED ASYNC FUNCTIONS =====");
    println!("hyper_bindgen: Generated {} async functions:", signatures.len());
    
    // Print organized by call type
    for (call_type, sigs) in by_call_type.iter() {
        println!("hyper_bindgen:   {} {} functions:", sigs.len(), call_type);
        for sig in sigs {
            let params = sig.params.iter()
                .map(|(name, type_name)| format!("{}: {}", name, type_name))
                .collect::<Vec<_>>()
                .join(", ");
                
            let fn_name = format!("{}_{}_rpc", sig.name, call_type);
            println!("hyper_bindgen:     {}({}) -> {}", fn_name, params, sig.return_type);
        }
    }
    
    // Print import example
    println!("\nhyper_bindgen: ===== HOW TO USE THESE FUNCTIONS =====");
    println!("hyper_bindgen: Import the async functions in your code:");
    println!("hyper_bindgen: use crate::hyperware_async::*;");
    println!("hyper_bindgen: ");
    println!("hyper_bindgen: Example usage:");
    if let Some(sig) = signatures.first() {
        let fn_name = format!("{}_{}_rpc", sig.name, sig.call_type);
        let addr_param = if sig.is_http {
            "\"http://example.com\"".to_string()
        } else {
            "Address::new(\"node\", \"process\", \"package\", \"publisher\")".to_string()
        };
        
        let param_values: Vec<String> = sig.params.iter()
            .map(|(name, typ)| {
                match typ.as_str() {
                    "String" => format!("\"example {}\"", name),
                    "i32" | "u32" | "i64" | "u64" => "42".to_string(),
                    "f32" | "f64" => "3.14".to_string(),
                    "bool" => "true".to_string(),
                    _ if typ.starts_with("Vec<") => "vec![]".to_string(),
                    _ if typ.starts_with("Option<") => "None".to_string(),
                    _ => "Default::default()".to_string(),
                }
            })
            .collect();
        
        let params = if param_values.is_empty() {
            addr_param
        } else {
            format!("{}, {}", addr_param, param_values.join(", "))
        };
        
        println!("hyper_bindgen: async fn my_function() {{");
        println!("hyper_bindgen:     let result = {}({}).await;", fn_name, params);
        println!("hyper_bindgen:     println!(\"Result: {{:?}}\", result);");
        println!("hyper_bindgen: }}");
    }
    println!("hyper_bindgen: =====================================");
}

// Parse a signature record name into function name and call type
fn parse_signature_record_name(record_name: &str) -> Option<(String, String)> {
    if record_name.contains("-signature-") {
        let parts: Vec<&str> = record_name.split("-signature-").collect();
        if parts.len() == 2 {
            return Some((parts[0].to_string(), parts[1].to_string()));
        }
    }
    None
}

// Convert WIT type to Rust type, with namespace awareness
fn wit_type_to_rust(wit_type: &str, interface_name: &str) -> String {
    match wit_type {
        "s32" => "i32".to_string(),
        "u32" => "u32".to_string(),
        "s64" => "i64".to_string(),
        "u64" => "u64".to_string(),
        "f32" => "f32".to_string(),
        "f64" => "f64".to_string(),
        "string" => "String".to_string(),
        "bool" => "bool".to_string(),
        "unit" => "()".to_string(),
        "address" => format!("hyperware_process_lib::Address"),
        _ if wit_type.starts_with("list<") && wit_type.ends_with(">") => {
            let inner_type = &wit_type[5..wit_type.len()-1];
            format!("Vec<{}>", wit_type_to_rust(inner_type, interface_name))
        },
        _ if wit_type.starts_with("option<") && wit_type.ends_with(">") => {
            let inner_type = &wit_type[7..wit_type.len()-1];
            format!("Option<{}>", wit_type_to_rust(inner_type, interface_name))
        },
        _ if wit_type.starts_with("result<") && wit_type.ends_with(">") => {
            // For result types, we only care about the success type (first param)
            if let Some(comma_pos) = wit_type.find(',') {
                let success_type = &wit_type[7..comma_pos];
                wit_type_to_rust(success_type.trim(), interface_name)
            } else {
                "()".to_string()
            }
        },
        _ if wit_type.starts_with("tuple<") && wit_type.ends_with(">") => {
            let inner_types = &wit_type[6..wit_type.len()-1];
            let rust_types: Vec<String> = inner_types.split(',')
                .map(|ty| wit_type_to_rust(ty.trim(), interface_name))
                .collect();
            format!("({})", rust_types.join(", "))
        },
        _ => {
            // For custom types, convert kebab-case to PascalCase with interface namespace
            let pascal_case = kebab_to_pascal(wit_type);
            
            // Handle special cases - custom types
            let snake_interface = kebab_to_snake(interface_name);
            format!("crate::hyperware::process::{}::{}", snake_interface, pascal_case)
        }
    }
}

// Convert kebab-case to snake_case (for function/parameter names)
fn kebab_to_snake(kebab: &str) -> String {
    kebab.replace('-', "_")
}

// Convert kebab-case to PascalCase (for type names)
fn kebab_to_pascal(kebab: &str) -> String {
    kebab.split('-')
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect()
}

// Generate default value for a Rust type
fn generate_default_value(rust_type: &str) -> proc_macro2::TokenStream {
    match rust_type {
        "i32" | "u32" | "i64" | "u64" => quote! { 0 },
        "f32" | "f64" => quote! { 0.0 },
        "String" => quote! { String::new() },
        "bool" => quote! { false },
        "()" => quote! { () },
        _ if rust_type.starts_with("Vec<") => quote! { Vec::new() },
        _ if rust_type.starts_with("Option<") => quote! { None },
        _ if rust_type.starts_with("(") && rust_type.ends_with(")") => {
            // For tuples, generate default values for each element
            let inner_types = &rust_type[1..rust_type.len()-1];
            let default_values: Vec<proc_macro2::TokenStream> = inner_types.split(',')
                .map(|ty| generate_default_value(ty.trim()))
                .collect();
            
            quote! { (#(#default_values),*) }
        },
        _ => {
            // For custom types, use Default::default()
            quote! { Default::default() }
        }
    }
}

// Generate the async functions 
fn generate_async_functions(signatures: &[FunctionSignature]) -> proc_macro2::TokenStream {
    let mut function_impls = Vec::new();
    
    for sig in signatures {
        // Add "_rpc" suffix and call type to function name to avoid duplicates
        let fn_name = format_ident!("{}_{}_rpc", &sig.name, &sig.call_type);
        let call_type = &sig.call_type;
        
        // Generate function parameters
        let mut param_defs = Vec::new();
        
        // Generate the target parameter based on call type
        let target_param = if sig.is_http {
            quote! { target: String }
        } else {
            // Always use the hyperware_process_lib::Address
            quote! { target: hyperware_process_lib::Address }
        };
        
        param_defs.push(target_param);
        
        let mut param_names = Vec::new();
        param_names.push(quote! { target });
        
        for (name, type_name) in &sig.params {
            let param_name = format_ident!("{}", name);
            
            // Handle param type - parse it and convert to tokens
            let param_type_tokens = match syn::parse_str::<syn::TypePath>(&type_name) {
                Ok(type_path) => {
                    // If it parses as a TypePath, use it directly
                    quote! { #type_path }
                },
                Err(_) => {
                    // Otherwise, try to create an identifier
                    let cleaned_type = type_name.replace("::", "_");
                    let param_type_ident = format_ident!("{}", cleaned_type);
                    quote! { #param_type_ident }
                }
            };
            
            param_defs.push(quote! { #param_name: #param_type_tokens });
            param_names.push(quote! { #param_name });
        }
        
        // Generate return type
        let return_type_tokens = match syn::parse_str::<syn::TypePath>(&sig.return_type) {
            Ok(type_path) => {
                // If it parses as a TypePath, use it directly
                quote! { #type_path }
            },
            Err(_) => {
                // Otherwise, try to create an identifier
                let cleaned_type = sig.return_type.replace("::", "_");
                let return_type_ident = format_ident!("{}", cleaned_type);
                quote! { #return_type_ident }
            }
        };
        
        // Generate default return value
        let default_return = generate_default_value(&sig.return_type);
        
        // Generate function docstring
        let doc_comment = format!("Async RPC function for {}-{}", sig.original_name, call_type);
        let original_fn_name = &sig.name;
        
        // Generate function implementation based on call type
        let fn_impl = match call_type.as_str() {
            "local" => {
                quote! {
                    #[doc = #doc_comment]
                    pub async fn #fn_name(#(#param_defs),*) -> #return_type_tokens {
                        // Default implementation for local call
                        println!("Local function called: {}", stringify!(#original_fn_name));
                        #default_return
                    }
                }
            },
            "remote" => {
                quote! {
                    #[doc = #doc_comment]
                    pub async fn #fn_name(#(#param_defs),*) -> #return_type_tokens {
                        // Default implementation for remote call
                        println!("Remote function called: {}", stringify!(#original_fn_name));
                        #default_return
                    }
                }
            },
            "http" => {
                quote! {
                    #[doc = #doc_comment]
                    pub async fn #fn_name(#(#param_defs),*) -> #return_type_tokens {
                        // Default implementation for HTTP call
                        println!("HTTP function called: {}", stringify!(#original_fn_name));
                        #default_return
                    }
                }
            },
            _ => continue,  // Skip unknown call types
        };
        
        function_impls.push(fn_impl);
    }
    
    // If no functions are found, generate an empty module with a comment
    if function_impls.is_empty() {
        return quote! {
            /// This module contains generated async RPC functions.
            /// No functions were found in the WIT files.
            pub mod hyperware_async {
                // No functions were generated
                #[allow(dead_code)]
                fn no_functions_found() {
                    println!("No async functions were generated. Check your WIT files for signature records.")
                }
            }
        };
    }
    
    // Combine all functions into a module with the appropriate documentation
    quote! {
        /// This module contains generated async RPC functions based on WIT signature records.
        /// These functions can be used to communicate with other processes in the Hyperware system.
        pub mod hyperware_async {
            #(#function_impls)*
        }
    }
}