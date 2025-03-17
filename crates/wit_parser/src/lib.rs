use nom::{
    branch::alt,
    bytes::complete::{tag, take_while, take_while1},
    character::complete::{alpha1, alphanumeric1, char, multispace0, multispace1},
    combinator::{map, opt, recognize},
    multi::{many0, separated_list0},
    sequence::{delimited, pair, preceded, tuple},
    IResult,
};
use proc_macro::TokenStream;
use proc_macro2::Span;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use syn::{
    parse::{Parse, ParseStream},
    LitBool, LitStr, Result as SynResult, Token,
};

extern crate proc_macro;

//------------------------------------------------------------------------------
// Macro Parser
//------------------------------------------------------------------------------

struct WitParserArgs {
    file_path: String,
    debug: bool,
}

impl Parse for WitParserArgs {
    fn parse(input: ParseStream) -> SynResult<Self> {
        // Parse the file path
        let file_path_lit = input.parse::<LitStr>()?;
        let file_path = file_path_lit.value();

        // Check if there are more arguments
        let debug = if input.peek(Token![,]) {
            // Consume the comma
            input.parse::<Token![,]>()?;

            // Parse debug = true/false
            let debug_ident = input.parse::<syn::Ident>()?;

            // Make sure the identifier is "debug"
            if debug_ident != "debug" {
                return Err(syn::Error::new(
                    debug_ident.span(),
                    format!("Expected 'debug', found '{}'", debug_ident),
                ));
            }

            input.parse::<Token![=]>()?;
            let debug_lit = input.parse::<LitBool>()?;
            debug_lit.value
        } else {
            false // Default to debug disabled
        };

        Ok(WitParserArgs { file_path, debug })
    }
}

//------------------------------------------------------------------------------
// Utility Functions
//------------------------------------------------------------------------------

// Convert kebab-case or snake_case to CamelCase (PascalCase)
fn to_camel_case(s: &str) -> String {
    let parts: Vec<&str> = s.split(|c| c == '-' || c == '_').collect();
    parts
        .iter()
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

//------------------------------------------------------------------------------
// AST Definitions
//------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum WitType {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Char,
    Bool,
    String,
    List(Box<WitType>),
    Option(Box<WitType>),
    Result(Box<WitType>, Box<WitType>),
    Record(Vec<RecordField>),
    Variant(Vec<VariantCase>),
    Enum(Vec<String>),
    Tuple(Vec<WitType>),
    Named(String),
}

#[derive(Debug, Clone, PartialEq)]
struct RecordField {
    name: String,
    typ: WitType,
}

#[derive(Debug, Clone, PartialEq)]
struct VariantCase {
    name: String,
    typ: Option<WitType>,
}

#[derive(Debug, Clone, PartialEq)]
struct TypeDef {
    name: String,
    typ: WitType,
}

#[derive(Debug, Clone, PartialEq)]
struct Function {
    name: String,
    params: Vec<Parameter>,
    results: Option<WitType>,
}

#[derive(Debug, Clone, PartialEq)]
struct Parameter {
    name: String,
    typ: WitType,
}

#[derive(Debug, Clone, PartialEq)]
struct Interface {
    name: String,
    types: Vec<TypeDef>,
    functions: Vec<Function>,
    use_statements: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum WorldItem {
    Import(String),
    Export(Interface),
    Include(String),
}

#[derive(Debug, Clone, PartialEq)]
struct World {
    name: String,
    items: Vec<WorldItem>,
}

//------------------------------------------------------------------------------
// Lexer Functions
//------------------------------------------------------------------------------

// Parse identifiers (alphanumeric strings with dashes, starting with a letter or underscore)
fn identifier(input: &str) -> IResult<&str, &str> {
    recognize(pair(
        alt((alpha1, tag("_"))),
        many0(alt((alphanumeric1, tag("_"), tag("-")))),
    ))(input)
}

// Helper function to wrap a parser with optional whitespace
fn ws<'a, F, O>(inner: F) -> impl FnMut(&'a str) -> IResult<&'a str, O>
where
    F: FnMut(&'a str) -> IResult<&'a str, O>,
{
    delimited(multispace0, inner, multispace0)
}

// Parse a comment line starting with //
fn comment_line(input: &str) -> IResult<&str, &str> {
    preceded(tag("//"), take_while(|c| c != '\n'))(input)
}

// Skip whitespace and comments
fn skip_ws_and_comments(input: &str) -> IResult<&str, ()> {
    let (input, _) = many0(alt((map(multispace1, |_| ()), map(comment_line, |_| ()))))(input)?;

    Ok((input, ()))
}

//------------------------------------------------------------------------------
// Parser Functions
//------------------------------------------------------------------------------

// Parse primitive types
fn parse_primitive_type(input: &str) -> IResult<&str, WitType> {
    alt((
        map(tag("i8"), |_| WitType::I8),
        map(tag("i16"), |_| WitType::I16),
        map(tag("i32"), |_| WitType::I32),
        map(tag("i64"), |_| WitType::I64),
        map(tag("u8"), |_| WitType::U8),
        map(tag("u16"), |_| WitType::U16),
        map(tag("u32"), |_| WitType::U32),
        map(tag("u64"), |_| WitType::U64),
        map(tag("f32"), |_| WitType::F32),
        map(tag("f64"), |_| WitType::F64),
        map(tag("char"), |_| WitType::Char),
        map(tag("bool"), |_| WitType::Bool),
        map(tag("string"), |_| WitType::String),
        map(tag("s32"), |_| WitType::I32), // Support for s32 syntax
        map(tag("s64"), |_| WitType::I64), // Support for s64 syntax
    ))(input)
}

// Parse container types
fn parse_container_type(input: &str) -> IResult<&str, WitType> {
    alt((
        map(delimited(ws(tag("list<")), parse_type, ws(tag(">"))), |t| {
            WitType::List(Box::new(t))
        }),
        map(
            delimited(ws(tag("option<")), parse_type, ws(tag(">"))),
            |t| WitType::Option(Box::new(t)),
        ),
        map(
            delimited(
                ws(tag("result<")),
                tuple((parse_type, opt(preceded(ws(tag(",")), parse_type)))),
                ws(tag(">")),
            ),
            |(ok, err)| {
                WitType::Result(
                    Box::new(ok),
                    Box::new(err.unwrap_or(WitType::Named("void".to_string()))),
                )
            },
        ),
    ))(input)
}

// Parse complex types
fn parse_complex_type(input: &str) -> IResult<&str, WitType> {
    alt((
        // Record type
        map(
            delimited(
                ws(char('{')),
                separated_list0(
                    ws(char(',')),
                    map(
                        tuple((ws(identifier), ws(char(':')), ws(parse_type))),
                        |(name, _, typ)| RecordField {
                            name: name.to_string(),
                            typ,
                        },
                    ),
                ),
                ws(char('}')),
            ),
            WitType::Record,
        ),
        // Variant type
        map(
            delimited(
                ws(tag("variant {")),
                separated_list0(
                    ws(char(',')),
                    alt((
                        map(
                            tuple((ws(identifier), ws(char('(')), ws(parse_type), ws(char(')')))),
                            |(name, _, typ, _)| VariantCase {
                                name: name.to_string(),
                                typ: Some(typ),
                            },
                        ),
                        map(ws(identifier), |name| VariantCase {
                            name: name.to_string(),
                            typ: None,
                        }),
                    )),
                ),
                ws(char('}')),
            ),
            WitType::Variant,
        ),
        // Enum type
        map(
            delimited(
                ws(tag("enum {")),
                separated_list0(ws(char(',')), ws(identifier)),
                ws(char('}')),
            ),
            |cases| WitType::Enum(cases.into_iter().map(String::from).collect()),
        ),
        // Tuple type
        map(
            delimited(
                ws(char('(')),
                separated_list0(ws(char(',')), ws(parse_type)),
                ws(char(')')),
            ),
            WitType::Tuple,
        ),
    ))(input)
}

// Parse a Wit type
fn parse_type(input: &str) -> IResult<&str, WitType> {
    alt((
        parse_primitive_type,
        parse_container_type,
        parse_complex_type,
        // Named type (reference)
        map(ws(identifier), |name| WitType::Named(name.to_string())),
    ))(input)
}

// Parse a type definition
fn parse_type_def(input: &str) -> IResult<&str, TypeDef> {
    map(
        tuple((
            ws(tag("type")),
            ws(identifier),
            ws(char('=')),
            ws(parse_type),
        )),
        |(_, name, _, typ)| TypeDef {
            name: name.to_string(),
            typ,
        },
    )(input)
}

// Parse a 'record' declaration
fn parse_record_def(input: &str) -> IResult<&str, TypeDef> {
    map(
        tuple((
            ws(tag("record")),
            ws(identifier),
            ws(char('{')),
            separated_list0(
                ws(char(',')),
                map(
                    tuple((ws(identifier), ws(char(':')), ws(parse_type))),
                    |(name, _, typ)| RecordField {
                        name: name.to_string(),
                        typ,
                    },
                ),
            ),
            ws(char('}')),
        )),
        |(_, name, _, fields, _)| TypeDef {
            name: name.to_string(),
            typ: WitType::Record(fields),
        },
    )(input)
}

// Parse a 'variant' declaration
fn parse_variant_def(input: &str) -> IResult<&str, TypeDef> {
    map(
        tuple((
            ws(tag("variant")),
            ws(identifier),
            ws(char('{')),
            separated_list0(
                ws(char(',')),
                alt((
                    map(
                        tuple((ws(identifier), ws(char('(')), ws(parse_type), ws(char(')')))),
                        |(name, _, typ, _)| VariantCase {
                            name: name.to_string(),
                            typ: Some(typ),
                        },
                    ),
                    map(ws(identifier), |name| VariantCase {
                        name: name.to_string(),
                        typ: None,
                    }),
                )),
            ),
            ws(char('}')),
        )),
        |(_, name, _, cases, _)| TypeDef {
            name: name.to_string(),
            typ: WitType::Variant(cases),
        },
    )(input)
}

// Parse a function parameter
fn parse_parameter(input: &str) -> IResult<&str, Parameter> {
    map(
        tuple((ws(identifier), ws(char(':')), ws(parse_type))),
        |(name, _, typ)| Parameter {
            name: name.to_string(),
            typ,
        },
    )(input)
}

// Parse a function
fn parse_function(input: &str) -> IResult<&str, Function> {
    map(
        tuple((
            ws(tag("func")),
            ws(identifier),
            ws(char('(')),
            separated_list0(ws(char(',')), ws(parse_parameter)),
            ws(char(')')),
            opt(preceded(ws(tag("->")), ws(parse_type))),
        )),
        |(_, name, _, params, _, results)| Function {
            name: name.to_string(),
            params,
            results,
        },
    )(input)
}

// Parse a use statement
fn parse_use_statement(input: &str) -> IResult<&str, String> {
    map(
        tuple((ws(tag("use")), ws(take_while1(|c| c != ';')), ws(char(';')))),
        |(_, import_path, _)| import_path.trim().to_string(),
    )(input)
}

// Parse an include statement
fn _parse_include_statement(input: &str) -> IResult<&str, String> {
    map(
        tuple((ws(tag("include")), ws(identifier), ws(char(';')))),
        |(_, name, _)| name.to_string(),
    )(input)
}

// Parse an interface
fn parse_interface(input: &str) -> IResult<&str, Interface> {
    let (input, _) = skip_ws_and_comments(input)?;

    map(
        tuple((
            ws(tag("interface")),
            ws(identifier),
            ws(char('{')),
            many0(alt((
                map(preceded(skip_ws_and_comments, parse_use_statement), |u| {
                    (Some(u), None, None)
                }),
                map(preceded(skip_ws_and_comments, parse_type_def), |t| {
                    (None, Some(t), None)
                }),
                map(preceded(skip_ws_and_comments, parse_record_def), |t| {
                    (None, Some(t), None)
                }),
                map(preceded(skip_ws_and_comments, parse_variant_def), |t| {
                    (None, Some(t), None)
                }),
                map(preceded(skip_ws_and_comments, parse_function), |f| {
                    (None, None, Some(f))
                }),
            ))),
            ws(char('}')),
        )),
        |(_, name, _, items, _)| {
            let mut types = Vec::new();
            let functions = Vec::new();
            let mut use_statements = Vec::new();

            for item in items {
                match item {
                    (Some(u), None, None) => use_statements.push(u),
                    (None, Some(t), None) => {
                        // Skip any type with "signature" in the name
                        if !t.name.contains("signature") {
                            types.push(t);
                        }
                    }
                    (None, None, Some(_)) => {
                        // Skip functions as requested
                    }
                    _ => {}
                }
            }

            Interface {
                name: name.to_string(),
                types,
                functions,
                use_statements,
            }
        },
    )(input)
}

// Parse a simple import statement in a world
fn parse_world_import(input: &str) -> IResult<&str, WorldItem> {
    map(
        tuple((ws(tag("import")), ws(identifier), ws(char(';')))),
        |(_, name, _)| WorldItem::Import(name.to_string()),
    )(input)
}

// Parse an include statement in a world
fn parse_world_include(input: &str) -> IResult<&str, WorldItem> {
    map(
        tuple((ws(tag("include")), ws(identifier), ws(char(';')))),
        |(_, name, _)| WorldItem::Include(name.to_string()),
    )(input)
}

// Parse a world definition
fn parse_world(input: &str) -> IResult<&str, World> {
    let (input, _) = skip_ws_and_comments(input)?;

    map(
        tuple((
            ws(tag("world")),
            ws(identifier),
            ws(char('{')),
            many0(alt((
                preceded(skip_ws_and_comments, parse_world_import),
                preceded(skip_ws_and_comments, parse_world_include),
                map(
                    tuple((ws(tag("export")), ws(parse_interface))),
                    |(_, interface)| WorldItem::Export(interface),
                ),
            ))),
            ws(char('}')),
        )),
        |(_, name, _, items, _)| World {
            name: name.to_string(),
            items,
        },
    )(input)
}

// Parse a complete Wit file
fn parse_wit_file(input: &str) -> IResult<&str, Vec<World>> {
    let (input, _) = skip_ws_and_comments(input)?;
    many0(preceded(skip_ws_and_comments, parse_world))(input)
}

// Parse interfaces from a complete Wit file
fn parse_interfaces(input: &str) -> IResult<&str, Vec<Interface>> {
    let (input, _) = skip_ws_and_comments(input)?;
    many0(preceded(skip_ws_and_comments, parse_interface))(input)
}

//------------------------------------------------------------------------------
// Code Generation Functions
//------------------------------------------------------------------------------

// Track generated types for debug mode
struct TypeTracker {
    debug: bool,
    types: Vec<String>,
}

impl TypeTracker {
    fn new(debug: bool) -> Self {
        TypeTracker {
            debug,
            types: Vec::new(),
        }
    }

    fn track(&mut self, type_name: &str, kind: &str) {
        if self.debug {
            self.types.push(format!("{} ({})", type_name, kind));
        }
    }

    fn print_summary(&self) {
        if self.debug {
            // Always print to stderr to make sure it's visible
            eprintln!("\n=== WIT Parser: Generated Types ===");

            if self.types.is_empty() {
                eprintln!("  No types were generated - the Wit file might be empty or not properly parsed");
            } else {
                for (i, typ) in self.types.iter().enumerate() {
                    eprintln!("  {}. {}", i + 1, typ);
                }
            }

            eprintln!("=== End of Generated Types ===\n");
        }
    }
}

// Generate Rust code for a single type
fn generate_type(typ: &WitType, output: &mut String) -> std::fmt::Result {
    match typ {
        WitType::I8 => write!(output, "i8"),
        WitType::I16 => write!(output, "i16"),
        WitType::I32 => write!(output, "i32"),
        WitType::I64 => write!(output, "i64"),
        WitType::U8 => write!(output, "u8"),
        WitType::U16 => write!(output, "u16"),
        WitType::U32 => write!(output, "u32"),
        WitType::U64 => write!(output, "u64"),
        WitType::F32 => write!(output, "f32"),
        WitType::F64 => write!(output, "f64"),
        WitType::Char => write!(output, "char"),
        WitType::Bool => write!(output, "bool"),
        WitType::String => write!(output, "String"),
        WitType::List(inner) => {
            write!(output, "Vec<")?;
            generate_type(inner, output)?;
            write!(output, ">")
        }
        WitType::Option(inner) => {
            write!(output, "Option<")?;
            generate_type(inner, output)?;
            write!(output, ">")
        }
        WitType::Result(ok, err) => {
            write!(output, "Result<")?;
            generate_type(ok, output)?;
            write!(output, ", ")?;
            generate_type(err, output)?;
            write!(output, ">")
        }
        WitType::Record(_) => {
            // Generated later as a struct
            write!(output, "/* record type */")
        }
        WitType::Variant(_) => {
            // Generated later as an enum
            write!(output, "/* variant type */")
        }
        WitType::Enum(_) => {
            // Generated later as an enum
            write!(output, "/* enum type */")
        }
        WitType::Tuple(types) => {
            write!(output, "(")?;
            for (i, typ) in types.iter().enumerate() {
                if i > 0 {
                    write!(output, ", ")?;
                }
                generate_type(typ, output)?;
            }
            write!(output, ")")
        }
        WitType::Named(name) => {
            // If this is "address", we need to handle it specially
            if name == "address" {
                write!(output, "String")
            } else {
                // Convert kebab-case to snake_case for fields, but to PascalCase for types
                let rust_name = to_camel_case(name);
                write!(output, "{}", rust_name)
            }
        }
    }
}

// Generate a Rust struct from a record type
fn generate_record(
    name: &str,
    fields: &[RecordField],
    output: &mut String,
    tracker: &mut TypeTracker,
) -> std::fmt::Result {
    // Skip generation if the name is "address"
    if name == "address" {
        return Ok(());
    }

    // Convert to CamelCase for struct name
    let struct_name = to_camel_case(name);

    writeln!(
        output,
        "#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, SerdeJsonInto)]"
    )?;
    writeln!(output, "pub struct {} {{", struct_name)?;
    for field in fields {
        let field_name = field.name.replace('-', "_");
        write!(output, "    pub {}: ", field_name)?;
        generate_type(&field.typ, output)?;
        writeln!(output, ",")?;
    }
    writeln!(output, "}}")?;

    tracker.track(name, "struct");
    Ok(())
}

// Generate a Rust enum from a variant type
fn generate_variant(
    name: &str,
    cases: &[VariantCase],
    output: &mut String,
    tracker: &mut TypeTracker,
) -> std::fmt::Result {
    // Skip generation if the name is "address"
    if name == "address" {
        return Ok(());
    }

    // Convert to CamelCase for enum name
    let enum_name = to_camel_case(name);

    writeln!(
        output,
        "#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, SerdeJsonInto)]"
    )?;
    writeln!(output, "pub enum {} {{", enum_name)?;
    for case in cases {
        // Convert kebab-case to PascalCase for enum variants
        let variant_name = to_camel_case(&case.name);

        write!(output, "    {}", variant_name)?;
        if let Some(typ) = &case.typ {
            write!(output, "(")?;
            generate_type(typ, output)?;
            write!(output, ")")?;
        }
        writeln!(output, ",")?;
    }
    writeln!(output, "}}")?;

    tracker.track(name, "variant enum");
    Ok(())
}

// Generate a Rust enum from an enum type
fn generate_enum(
    name: &str,
    cases: &[String],
    output: &mut String,
    tracker: &mut TypeTracker,
) -> std::fmt::Result {
    // Skip generation if the name is "address"
    if name == "address" {
        return Ok(());
    }

    // Convert to CamelCase for enum name
    let enum_name = to_camel_case(name);

    writeln!(
        output,
        "#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, SerdeJsonInto)]"
    )?;
    writeln!(output, "pub enum {} {{", enum_name)?;
    for case in cases {
        // Convert kebab-case to PascalCase for enum variants
        let variant_name = to_camel_case(case);
        writeln!(output, "    {},", variant_name)?;
    }
    writeln!(output, "}}")?;

    tracker.track(name, "enum");
    Ok(())
}

// Generate Rust code for a type definition
fn generate_typedef(
    typedef: &TypeDef,
    output: &mut String,
    tracker: &mut TypeTracker,
) -> std::fmt::Result {
    // Skip generation if the name is "address"
    if typedef.name == "address" {
        return Ok(());
    }

    // Convert to CamelCase for type names
    let type_name = to_camel_case(&typedef.name);

    match &typedef.typ {
        WitType::Record(fields) => generate_record(&typedef.name, fields, output, tracker),
        WitType::Variant(cases) => generate_variant(&typedef.name, cases, output, tracker),
        WitType::Enum(cases) => generate_enum(&typedef.name, cases, output, tracker),
        _ => {
            write!(output, "pub type {} = ", type_name)?;
            generate_type(&typedef.typ, output)?;
            writeln!(output, ";")?;

            tracker.track(&typedef.name, "type alias");
            Ok(())
        }
    }
}

// Generate Rust code for an interface
fn generate_interface(
    interface: &Interface,
    output: &mut String,
    tracker: &mut TypeTracker,
) -> std::fmt::Result {
    // Handle use statements from the interface
    if !interface.use_statements.is_empty() {
        // Process standard.{} imports but skip generating 'address' type
        for use_stmt in &interface.use_statements {
            if use_stmt.starts_with("standard.") {
                let parts: Vec<&str> = use_stmt.split('{').collect();
                if parts.len() > 1 {
                    let inner = parts[1].trim_end_matches('}').trim();
                    let types: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();

                    for typ in types {
                        // Skip generating 'address' type
                        if typ != "address" {
                            let rust_type = match typ {
                                // Handle any other special mappings here
                                _ => typ,
                            };

                            writeln!(output, "pub type {} = {};", typ, rust_type)?;
                            tracker.track(typ, "type alias from standard");
                        }
                    }
                }
                writeln!(output)?;
            }
        }
    }

    // Generate types
    for typedef in &interface.types {
        // Skip 'address' type
        if typedef.name != "address" {
            generate_typedef(typedef, output, tracker)?;
            writeln!(output)?;
        }
    }

    Ok(())
}

// Find and parse a specific interface file
fn parse_interface_file(interface_name: &str, base_dir: &Path, debug: bool) -> Option<Interface> {
    let file_path = base_dir.join(format!("{}.wit", interface_name));

    if debug {
        eprintln!("Looking for interface file: {}", file_path.display());
    }

    // Try to read the file
    match fs::read_to_string(&file_path) {
        Ok(content) => {
            if debug {
                eprintln!("Found interface file: {}", file_path.display());
            }

            // Parse the interface
            match parse_interface(&content) {
                Ok((_, interface)) => {
                    if debug {
                        eprintln!("Successfully parsed interface: {}", interface_name);
                    }
                    Some(interface)
                }
                Err(e) => {
                    if debug {
                        eprintln!(
                            "Error parsing interface file {}: {:?}",
                            file_path.display(),
                            e
                        );
                    }
                    None
                }
            }
        }
        Err(_) => None,
    }
}

// Generate Rust code from parsed Wit file
fn generate_code(file_path: &str, debug: bool) -> String {
    let mut output = String::new();
    let mut tracker = TypeTracker::new(debug);

    writeln!(&mut output, "// Generated from Wit file: {}", file_path).unwrap();
    writeln!(&mut output, "// DO NOT EDIT - GENERATED CODE").unwrap();
    writeln!(&mut output).unwrap();

    // Start with a single top-level module named "wit_custom"
    writeln!(&mut output, "pub mod wit_custom {{").unwrap();
    writeln!(&mut output, "    use std::rc::Rc;").unwrap();
    writeln!(&mut output, "    use std::collections::HashMap;").unwrap();
    writeln!(&mut output, "    use serde::{{Deserialize, Serialize}};").unwrap();
    writeln!(&mut output, "    use process_macros::SerdeJsonInto;").unwrap();
    writeln!(&mut output).unwrap();

    // Get the base directory for looking up related files
    let file_path_buf = PathBuf::from(file_path);
    let base_dir = file_path_buf.parent().unwrap_or_else(|| Path::new("."));

    // Read the content of the file
    match fs::read_to_string(file_path) {
        Ok(content) => {
            if debug {
                eprintln!("Processing Wit file content: {} bytes", content.len());
            }

            // First try to parse worlds
            let result = parse_wit_file(&content);
            match result {
                Ok((_, worlds)) => {
                    if !worlds.is_empty() {
                        if debug {
                            eprintln!(
                                "Successfully parsed {} world(s) from Wit file",
                                worlds.len()
                            );
                        }

                        for world in &worlds {
                            // Process imports and includes
                            let mut imports = Vec::new();
                            let mut includes = Vec::new();
                            let mut exports = Vec::new();

                            for item in &world.items {
                                match item {
                                    WorldItem::Import(name) => {
                                        imports.push(name);
                                    }
                                    WorldItem::Include(name) => {
                                        includes.push(name);
                                    }
                                    WorldItem::Export(interface) => {
                                        exports.push(interface);
                                    }
                                }
                            }

                            // Generate all imported interfaces
                            for import_name in &imports {
                                if let Some(interface) =
                                    parse_interface_file(import_name, base_dir, debug)
                                {
                                    writeln!(&mut output, "    // From interface: {}", import_name)
                                        .unwrap();
                                    generate_interface(&interface, &mut output, &mut tracker)
                                        .unwrap();
                                } else if debug {
                                    eprintln!("Could not find or parse interface: {}", import_name);
                                }
                            }

                            // Handle includes if needed (currently ignored)

                            // Generate exports
                            for interface in exports {
                                writeln!(
                                    &mut output,
                                    "    // Export interface: {}",
                                    interface.name
                                )
                                .unwrap();
                                generate_interface(interface, &mut output, &mut tracker).unwrap();
                            }
                        }
                    } else {
                        // Try to parse standalone interfaces if no worlds were found
                        if debug {
                            eprintln!("No worlds found, trying to parse standalone interfaces");
                        }

                        let interfaces_result = parse_interfaces(&content);
                        match interfaces_result {
                            Ok((_, interfaces)) => {
                                if !interfaces.is_empty() {
                                    if debug {
                                        eprintln!(
                                            "Found {} standalone interface(s)",
                                            interfaces.len()
                                        );
                                    }

                                    for interface in &interfaces {
                                        writeln!(
                                            &mut output,
                                            "    // Standalone interface: {}",
                                            interface.name
                                        )
                                        .unwrap();
                                        generate_interface(interface, &mut output, &mut tracker)
                                            .unwrap();
                                    }
                                } else if debug {
                                    eprintln!("No interfaces found either. Wit file might be empty or have an unsupported format.");
                                }
                            }
                            Err(e) => {
                                if debug {
                                    eprintln!("Error parsing interfaces: {:?}", e);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    if debug {
                        eprintln!("Error parsing worlds: {:?}", e);
                        eprintln!("Trying to parse standalone interfaces instead");
                    }

                    // Try to parse standalone interfaces if world parsing failed
                    let interfaces_result = parse_interfaces(&content);
                    match interfaces_result {
                        Ok((_, interfaces)) => {
                            if !interfaces.is_empty() {
                                if debug {
                                    eprintln!("Found {} standalone interface(s)", interfaces.len());
                                }

                                for interface in &interfaces {
                                    writeln!(
                                        &mut output,
                                        "    // Standalone interface: {}",
                                        interface.name
                                    )
                                    .unwrap();
                                    generate_interface(interface, &mut output, &mut tracker)
                                        .unwrap();
                                }
                            } else if debug {
                                eprintln!("No interfaces found either. Wit file might be empty or have an unsupported format.");
                            }
                        }
                        Err(e) => {
                            if debug {
                                eprintln!("Error parsing interfaces: {:?}", e);
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            if debug {
                eprintln!("Error reading file: {}", e);
            }
        }
    }

    // Close the top-level module
    writeln!(&mut output, "}}").unwrap();

    // Print summary of generated types if debug is enabled
    tracker.print_summary();

    output
}

//------------------------------------------------------------------------------
// Macro Implementation
//------------------------------------------------------------------------------

/// Parses a Wit file at compile-time and generates the corresponding Rust types.
///
/// # Example
///
/// ```ignore
/// use wit_parser::wit_parser;
///
/// // Basic usage
/// wit_parser!("path/to/your.wit");
///
/// // With debug output enabled (shows generated types during compilation)
/// wit_parser!("path/to/your.wit", debug = true);
///
/// fn main() {
///     // Use the generated types here
/// }
/// ```
#[proc_macro]
pub fn wit_parser(input: TokenStream) -> TokenStream {
    // Parse the macro arguments
    let args = match syn::parse::<WitParserArgs>(input) {
        Ok(args) => args,
        Err(e) => return e.to_compile_error().into(),
    };

    // Force a print before reading the file to make sure stderr is being captured
    if args.debug {
        eprintln!("\n=== WIT Parser: Processing file {} ===", args.file_path);
    }

    // Generate the Rust code from the Wit file
    let generated_code = generate_code(&args.file_path, args.debug);

    // Return the generated code as a TokenStream
    match TokenStream::from_str(&generated_code) {
        Ok(tokens) => tokens,
        Err(e) => {
            let error = format!("Error converting generated code to tokens: {}", e);
            if args.debug {
                eprintln!("{}", error);
            }
            syn::Error::new(Span::call_site(), error)
                .to_compile_error()
                .into()
        }
    }
}
