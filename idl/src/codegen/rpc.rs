//! RPC type code generation (7.5.1.1)
//!
//! Generates Rust types from IDL interface definitions:
//! In/Out structs, Result/Call/Return unions, Request/Reply wrappers.

use crate::naming;
use crate::types::*;

/// HASH function (7.5.1.1.2)
///
/// Computes a 32-bit hash from the first 4 bytes of an MD5 digest (little-endian).
/// Used to generate discriminant values for Call/Return unions (operation hashes)
/// and Result unions (exception hashes).
pub fn rpc_hash(name: &str) -> i32 {
    let digest = md5::compute(name.as_bytes());
    let bytes = digest.0;
    i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

pub fn generate(model: &IdlModel, opts: &RpcOptions) -> String {
    let mut gen = RpcGen { out: String::new(), indent: 0, opts };

    if model.interfaces.is_empty() {
        return gen.out;
    }

    gen.line("#![allow(non_camel_case_types)]");
    gen.line("");
    gen.line(&format!("use {}::prelude::*;", opts.crate_path));
    gen.line(&format!("use {}_derive::DdsType;", opts.crate_path));
    gen.line(&format!("use {}_rpc::types::UnusedMember;", opts.crate_path));
    gen.line("");

    for iface in &model.interfaces {
        gen.emit_interface(iface);
    }

    gen.out
}

pub struct RpcOptions {
    pub crate_path: String,
}

impl Default for RpcOptions {
    fn default() -> Self {
        Self { crate_path: "int2dds".to_string() }
    }
}

struct RpcGen<'a> {
    out: String,
    indent: usize,
    opts: &'a RpcOptions,
}

impl<'a> RpcGen<'a> {
    fn emit_interface(&mut self, iface: &ResolvedInterface) {
        for op in &iface.operations {
            self.emit_in_struct(&iface.name, op);
            self.line("");
            self.emit_out_struct(&iface.name, op);
            self.line("");
        }
    }

    /// (7.5.1.1.4) Generate `${Interface}_${Operation}_In` struct.
    fn emit_in_struct(&mut self, iface_name: &str, op: &ResolvedOperation) {
        let struct_name = format!("{}_{}_In", iface_name, op.name);

        let in_params: Vec<&ResolvedParam> = op
            .params
            .iter()
            .filter(|p| matches!(p.direction, ResolvedParamDirection::In | ResolvedParamDirection::Inout))
            .collect();

        self.line("#[derive(DdsType)]");
        self.line(&format!("pub struct {} {{", struct_name));
        self.indent += 1;

        if in_params.is_empty() {
            self.line("pub dummy: UnusedMember,");
        } else {
            for p in &in_params {
                let type_str = self.type_to_rust(&p.resolved_type);
                self.line(&format!("pub {}: {},", p.name, type_str));
            }
        }

        self.indent -= 1;
        self.line("}");
    }

    /// (7.5.1.1.5) Generate `${Interface}_${Operation}_Out` struct.
    fn emit_out_struct(&mut self, iface_name: &str, op: &ResolvedOperation) {
        let struct_name = format!("{}_{}_Out", iface_name, op.name);

        let out_params: Vec<&ResolvedParam> = op
            .params
            .iter()
            .filter(|p| matches!(p.direction, ResolvedParamDirection::Out | ResolvedParamDirection::Inout))
            .collect();

        let has_return = op.return_type.is_some();
        let has_out_params = !out_params.is_empty();

        self.line("#[derive(DdsType)]");
        self.line(&format!("pub struct {} {{", struct_name));
        self.indent += 1;

        if !has_return && !has_out_params {
            // (rule 3c) no return, no out/inout → dummy
            self.line("pub dummy: UnusedMember,");
        } else {
            // (rule 3a) out/inout params in order
            for p in &out_params {
                let type_str = self.type_to_rust(&p.resolved_type);
                self.line(&format!("pub {}: {},", p.name, type_str));
            }

            // (rule 3b) non-void return → last member named "return_"
            if let Some(ret_type) = &op.return_type {
                let return_name = Self::resolve_return_name(&out_params);
                let type_str = self.type_to_rust(ret_type);
                self.line(&format!("pub {}: {},", return_name, type_str));
            }
        }

        self.indent -= 1;
        self.line("}");
    }

    /// (7.5.1.1.5 rule 3b) Resolve `return_` name collision with out/inout params.
    /// If a param is already named `return_`, use `return_N` (N starts from 1).
    fn resolve_return_name(out_params: &[&ResolvedParam]) -> String {
        let base = "return_";
        if !out_params.iter().any(|p| p.name == base) {
            return base.to_string();
        }
        let mut n = 1u32;
        loop {
            let candidate = format!("return_{}", n);
            if !out_params.iter().any(|p| p.name == candidate) {
                return candidate;
            }
            n += 1;
        }
    }

    fn type_to_rust(&self, ty: &ResolvedType) -> String {
        match ty {
            ResolvedType::Bool => "bool".to_string(),
            ResolvedType::U8 => "u8".to_string(),
            ResolvedType::I8 => "i8".to_string(),
            ResolvedType::I16 => "i16".to_string(),
            ResolvedType::U16 => "u16".to_string(),
            ResolvedType::I32 => "i32".to_string(),
            ResolvedType::U32 => "u32".to_string(),
            ResolvedType::I64 => "i64".to_string(),
            ResolvedType::U64 => "u64".to_string(),
            ResolvedType::F32 => "f32".to_string(),
            ResolvedType::F64 => "f64".to_string(),
            ResolvedType::Char => "char".to_string(),
            ResolvedType::WChar => "WChar".to_string(),
            ResolvedType::String { .. } => "String".to_string(),
            ResolvedType::WString { .. } => "WString".to_string(),
            ResolvedType::Sequence { element, .. } => {
                format!("Vec<{}>", self.type_to_rust(element))
            }
            ResolvedType::Array { element, size } => {
                format!("[{}; {}]", self.type_to_rust(element), size)
            }
            ResolvedType::Map { key, value, .. } => {
                format!("HashMap<{}, {}>", self.type_to_rust(key), self.type_to_rust(value))
            }
            ResolvedType::Struct(name) | ResolvedType::Enum(name) | ResolvedType::Bitmask(name) => {
                let simple = name.rsplit("::").next().unwrap_or(name);
                naming::to_pascal_case(simple)
            }
        }
    }

    fn line(&mut self, text: &str) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
        self.out.push_str(text);
        self.out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_idl;
    use crate::resolver::resolve;

    #[test]
    fn hash_deterministic() {
        assert_eq!(rpc_hash("setSpeed"), rpc_hash("setSpeed"));
    }

    #[test]
    fn hash_different_names() {
        assert_ne!(rpc_hash("setSpeed"), rpc_hash("getSpeed"));
    }

    #[test]
    fn hash_empty_string() {
        // MD5("") = d41d8cd98f00b204e9800998ecf8427e
        let h = rpc_hash("");
        let expected = i32::from_le_bytes([0xd4, 0x1d, 0x8c, 0xd9]);
        assert_eq!(h, expected);
    }

    #[test]
    fn hash_uses_unqualified_name() {
        // (7.5.1.1.6) operation hash uses unqualified name
        assert_ne!(rpc_hash("command"), rpc_hash("setSpeed"));
    }

    #[test]
    fn test_in_struct_with_params() {
        let defs = parse_idl(
            r#"
            interface RobotControl {
                void setSpeed(in float speed);
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        assert!(code.contains("pub struct RobotControl_setSpeed_In {"));
        assert!(code.contains("pub speed: f32,"));
    }

    #[test]
    fn test_in_struct_no_params() {
        // (7.5.1.1.4 rule 3c) no in/inout params → UnusedMember dummy
        let defs = parse_idl(
            r#"
            interface RobotControl {
                float getSpeed();
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        assert!(code.contains("pub struct RobotControl_getSpeed_In {"));
        assert!(code.contains("pub dummy: UnusedMember,"));
    }

    /// Extract the body of a struct by name from generated code.
    fn extract_struct_body<'a>(code: &'a str, name: &str) -> &'a str {
        let header = format!("pub struct {} {{", name);
        let start = code.find(&header).unwrap_or_else(|| panic!("struct '{}' not found", name));
        let body_start = start + header.len();
        let end = body_start + code[body_start..].find('}').unwrap();
        &code[body_start..end]
    }

    #[test]
    fn test_in_struct_filters_out_params() {
        // Only in/inout params, not out params
        let defs = parse_idl(
            r#"
            interface Foo {
                void bar(in long a, out long b, inout long c);
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());
        let body = extract_struct_body(&code, "Foo_bar_In");

        assert!(body.contains("pub a: i32,"));
        assert!(!body.contains("pub b: i32,")); // out param excluded
        assert!(body.contains("pub c: i32,"));  // inout included
    }

    #[test]
    fn test_in_struct_param_order() {
        // (7.5.1.1.4 rule 3a) members in same order as params, left to right
        let defs = parse_idl(
            r#"
            interface Calc {
                void compute(in long x, inout double y, in string z);
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        let x_pos = code.find("pub x: i32,").unwrap();
        let y_pos = code.find("pub y: f64,").unwrap();
        let z_pos = code.find("pub z: String,").unwrap();
        assert!(x_pos < y_pos);
        assert!(y_pos < z_pos);
    }

    #[test]
    fn test_out_struct_void_no_out_params() {
        // (7.5.1.1.5 rule 3c) void return + no out/inout → dummy
        let defs = parse_idl(
            r#"
            interface Foo {
                void fire(in long x);
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        assert!(code.contains("pub struct Foo_fire_Out {"));
        assert!(code.contains("pub dummy: UnusedMember,"));
    }

    #[test]
    fn test_out_struct_with_return() {
        // (7.5.1.1.5 rule 3b) non-void return → last member "return_"
        let defs = parse_idl(
            r#"
            interface Foo {
                float getSpeed();
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        assert!(code.contains("pub struct Foo_getSpeed_Out {"));
        assert!(code.contains("pub return_: f32,"));
    }

    #[test]
    fn test_out_struct_with_out_params_and_return() {
        // out/inout params in order, then return_ last
        let defs = parse_idl(
            r#"
            interface Foo {
                long compute(in long x, out double y, inout string z);
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());
        let body = extract_struct_body(&code, "Foo_compute_Out");

        let y_pos = body.find("pub y: f64,").unwrap();
        let z_pos = body.find("pub z: String,").unwrap();
        let ret_pos = body.find("pub return_: i32,").unwrap();
        // out/inout params first, then return_ last
        assert!(y_pos < z_pos);
        assert!(z_pos < ret_pos);
    }

    #[test]
    fn test_out_struct_return_name_collision() {
        // (7.5.1.1.5 rule 3b) out param named "return_" → return value uses "return_1"
        let defs = parse_idl(
            r#"
            interface Foo {
                long bar(out long return_);
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        assert!(code.contains("pub return_: i32,")); // out param keeps its name
        assert!(code.contains("pub return_1: i32,")); // return value gets return_1
    }

    #[test]
    fn test_out_struct_only_out_params_no_return() {
        // void return + out params → out params only, no dummy
        let defs = parse_idl(
            r#"
            interface Foo {
                void getPosition(out float x, out float y);
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());
        let body = extract_struct_body(&code, "Foo_getPosition_Out");

        assert!(body.contains("pub x: f32,"));
        assert!(body.contains("pub y: f32,"));
        assert!(!body.contains("dummy"));
        assert!(!body.contains("return_"));
    }

    #[test]
    fn test_in_struct_multiple_operations() {
        let defs = parse_idl(
            r#"
            enum Command { CYCLIC, POSITION };
            interface RobotControl {
                void command(in Command com);
                void setSpeed(in float speed);
                float getSpeed();
                long getStatus();
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        // Each operation gets its own In struct
        assert!(code.contains("pub struct RobotControl_command_In {"));
        assert!(code.contains("pub com: Command,"));

        assert!(code.contains("pub struct RobotControl_setSpeed_In {"));
        assert!(code.contains("pub speed: f32,"));

        assert!(code.contains("pub struct RobotControl_getSpeed_In {"));
        assert!(code.contains("pub struct RobotControl_getStatus_In {"));

        // getSpeed and getStatus have no in params → dummy
        let get_speed_start = code.find("pub struct RobotControl_getSpeed_In {").unwrap();
        let get_speed_end = code[get_speed_start..].find('}').unwrap() + get_speed_start;
        let get_speed_body = &code[get_speed_start..get_speed_end];
        assert!(get_speed_body.contains("pub dummy: UnusedMember,"));
    }
}
