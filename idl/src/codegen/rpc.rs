//! RPC type code generation (7.5.1.1)
//!
//! Generates Rust types from IDL interface definitions:
//! In/Out structs, Result/Call/Return unions, Request/Reply wrappers.

use crate::naming;
use crate::naming::to_snake_case;
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
    gen.line(&format!("use {}::DdsType;", opts.crate_path));
    gen.line(&format!(
        "use {}_rpc::types::{{UnusedMember, UnknownOperation, UnknownException, RequestHeader, ReplyHeader, RemoteExceptionCode}};",
        opts.crate_path
    ));
    gen.line(&format!("use {}_rpc::client::{{Client, ClientParams}};", opts.crate_path));
    gen.line(&format!(
        "use {}_rpc::service::{{Service, ServiceParams, RequestHandler}};",
        opts.crate_path
    ));
    gen.line(&format!("use {}_rpc::error::{{DdsRpcError, DdsRpcResult}};", opts.crate_path));
    gen.line(&format!("use {}_rpc::types::SampleIdentity;", opts.crate_path));
    gen.line("use std::time::Duration;");
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
        // (7.5.1.1.3) Expand attributes to implied operations
        let mut all_ops = iface.operations.clone();
        for attr in &iface.attributes {
            Self::validate_attribute_names(attr, &iface.operations);
            all_ops.extend(Self::expand_attribute(attr));
        }

        // Per-operation types: In, Out, Result
        for op in &all_ops {
            self.emit_in_struct(&iface.name, op);
            self.line("");
            self.emit_out_struct(&iface.name, op);
            self.line("");
            self.emit_result_union(&iface.name, op);
            self.line("");
        }

        // Operation hash constants (shared by Call and Return)
        self.emit_operation_hash_constants(&iface.name, &all_ops);
        self.line("");

        // Interface-level types
        self.emit_call_union(&iface.name, &all_ops);
        self.line("");
        self.emit_return_union(&iface.name, &all_ops);
        self.line("");
        self.emit_request_struct(&iface.name);
        self.line("");
        self.emit_reply_struct(&iface.name);
        self.line("");

        // Function-call style (7.11.1.5)
        self.emit_service_trait(&iface.name, &all_ops);
        self.line("");
        self.emit_service_dispatcher(&iface.name, &all_ops);
        self.line("");
        self.emit_client_struct(&iface.name, &all_ops);
        self.line("");
    }

    /// (7.5.1.1.3) Expand an attribute to getter/setter operations.
    fn expand_attribute(attr: &ResolvedAttribute) -> Vec<ResolvedOperation> {
        let mut ops = Vec::new();

        // getter: `get_attribute_<name>()` → returns attribute type, no params
        ops.push(ResolvedOperation {
            name: format!("get_attribute_{}", attr.name),
            return_type: Some(attr.resolved_type.clone()),
            params: vec![],
            raises: attr.raises.clone(),
        });

        // setter (unless readonly): `set_attribute_<name>(in <type> <name>)` → void
        if !attr.readonly {
            ops.push(ResolvedOperation {
                name: format!("set_attribute_{}", attr.name),
                return_type: None,
                params: vec![ResolvedParam {
                    name: attr.name.clone(),
                    resolved_type: attr.resolved_type.clone(),
                    direction: ResolvedParamDirection::In,
                }],
                raises: attr.raises.clone(),
            });
        }

        ops
    }

    /// (7.5.1.1.3 rule 1) Detect name collision between attribute-derived and user-defined operations.
    fn validate_attribute_names(attr: &ResolvedAttribute, operations: &[ResolvedOperation]) {
        let getter = format!("get_attribute_{}", attr.name);
        let setter = format!("set_attribute_{}", attr.name);
        for op in operations {
            if op.name == getter || op.name == setter {
                panic!(
                    "name collision: attribute '{}' conflicts with operation '{}'",
                    attr.name, op.name
                );
            }
        }
    }

    /// (7.5.1.1.4) Generate `${Interface}_${Operation}_In` struct.
    fn emit_in_struct(&mut self, iface_name: &str, op: &ResolvedOperation) {
        let struct_name = format!("{}_{}_In", iface_name, op.name);

        let in_params: Vec<&ResolvedParam> = op
            .params
            .iter()
            .filter(|p| {
                matches!(p.direction, ResolvedParamDirection::In | ResolvedParamDirection::Inout)
            })
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
            .filter(|p| {
                matches!(p.direction, ResolvedParamDirection::Out | ResolvedParamDirection::Inout)
            })
            .collect();

        let has_return = op.return_type.is_some();
        let has_out_params = !out_params.is_empty();

        self.line("#[derive(DdsType)]");
        self.line(&format!("pub struct {} {{", struct_name));
        self.indent += 1;

        if !has_return && !has_out_params {
            self.line("pub dummy: UnusedMember,");
        } else {
            for p in &out_params {
                let type_str = self.type_to_rust(&p.resolved_type);
                self.line(&format!("pub {}: {},", p.name, type_str));
            }

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

    /// (7.5.1.1.5) Generate `${Interface}_${Operation}_Result` union.
    fn emit_result_union(&mut self, iface_name: &str, op: &ResolvedOperation) {
        let union_name = format!("{}_{}_Result", iface_name, op.name);
        let out_type = format!("{}_{}_Out", iface_name, op.name);

        // (rule 4) exception hash constants
        for exc_name in &op.raises {
            let simple = exc_name.rsplit("::").next().unwrap_or(exc_name);
            let hash_const_name = format!("{}_EX_HASH", naming::to_screaming_snake(simple));
            let hash_value = rpc_hash(exc_name);
            self.line(&format!("pub const {}: i32 = {};", hash_const_name, hash_value));
        }
        if !op.raises.is_empty() {
            self.line("");
        }

        self.line("#[derive(DdsType)]");
        self.line("#[repr(i32)]");
        self.line(&format!("pub enum {} {{", union_name));
        self.indent += 1;

        self.line(&format!("Result({}) = 0,", out_type));

        for exc_name in &op.raises {
            let simple = exc_name.rsplit("::").next().unwrap_or(exc_name);
            let hash_const_name = format!("{}_EX_HASH", naming::to_screaming_snake(simple));
            let member_name = format!("{}_ex", naming::to_pascal_case(simple));
            let exc_type = naming::to_pascal_case(simple);
            self.line(&format!("{}({}) = {},", member_name, exc_type, hash_const_name));
        }

        self.indent -= 1;
        self.line("}");
    }

    /// (7.5.1.1.6 rule 4, 7.5.1.1.7 rule 4) Emit operation hash constants.
    /// Shared by Call and Return unions.
    fn emit_operation_hash_constants(&mut self, iface_name: &str, ops: &[ResolvedOperation]) {
        for op in ops {
            let const_name = format!(
                "{}_{}_HASH",
                naming::to_screaming_snake(iface_name),
                naming::to_screaming_snake(&op.name)
            );
            let hash_value = rpc_hash(&op.name);
            self.line(&format!("pub const {}: i32 = {};", const_name, hash_value));
        }
    }

    /// (7.5.1.1.6) Generate `${Interface}_Call` union.
    fn emit_call_union(&mut self, iface_name: &str, ops: &[ResolvedOperation]) {
        let union_name = format!("{}_Call", iface_name);

        self.line("#[derive(DdsType)]");
        self.line("#[repr(i32)]");
        self.line(&format!("pub enum {} {{", union_name));
        self.indent += 1;

        // (rule 3) default case
        self.line("UnknownOp(UnknownOperation) = -1,");

        // (rule 5) case for each operation
        for op in ops {
            let in_type = format!("{}_{}_In", iface_name, op.name);
            let const_name = format!(
                "{}_{}_HASH",
                naming::to_screaming_snake(iface_name),
                naming::to_screaming_snake(&op.name)
            );
            self.line(&format!(
                "{}({}) = {},",
                naming::to_pascal_case(&op.name),
                in_type,
                const_name
            ));
        }

        self.indent -= 1;
        self.line("}");
    }

    /// (7.5.1.1.7) Generate `${Interface}_Return` union.
    fn emit_return_union(&mut self, iface_name: &str, ops: &[ResolvedOperation]) {
        let union_name = format!("{}_Return", iface_name);

        self.line("#[derive(DdsType)]");
        self.line("#[repr(i32)]");
        self.line(&format!("pub enum {} {{", union_name));
        self.indent += 1;

        // (rule 3) default case
        self.line("UnknownOp(UnknownOperation) = -1,");

        // (rule 5) case for each operation
        for op in ops {
            let result_type = format!("{}_{}_Result", iface_name, op.name);
            let const_name = format!(
                "{}_{}_HASH",
                naming::to_screaming_snake(iface_name),
                naming::to_screaming_snake(&op.name)
            );
            self.line(&format!(
                "{}({}) = {},",
                naming::to_pascal_case(&op.name),
                result_type,
                const_name
            ));
        }

        self.indent -= 1;
        self.line("}");
    }

    /// (7.5.1.1.6) Generate `${Interface}_Request` struct.
    fn emit_request_struct(&mut self, iface_name: &str) {
        let struct_name = format!("{}_Request", iface_name);
        let call_type = format!("{}_Call", iface_name);

        self.line("#[derive(DdsType)]");
        self.line(&format!("pub struct {} {{", struct_name));
        self.indent += 1;
        self.line("pub header: RequestHeader,");
        self.line(&format!("pub data: {},", call_type));
        self.indent -= 1;
        self.line("}");
    }

    /// (7.5.1.1.7) Generate `${Interface}_Reply` struct.
    fn emit_reply_struct(&mut self, iface_name: &str) {
        let struct_name = format!("{}_Reply", iface_name);
        let return_type = format!("{}_Return", iface_name);

        self.line("#[derive(DdsType)]");
        self.line(&format!("pub struct {} {{", struct_name));
        self.indent += 1;
        self.line("pub header: ReplyHeader,");
        self.line(&format!("pub data: {},", return_type));
        self.indent -= 1;
        self.line("}");
    }

    /// (7.11.1.5) Generate service trait.
    /// Users implement this trait to provide the service logic.
    fn emit_service_trait(&mut self, iface_name: &str, ops: &[ResolvedOperation]) {
        self.line(&format!("pub trait {} {{", iface_name));
        self.indent += 1;

        for op in ops {
            let method_name = to_snake_case(&op.name);

            let mut params = String::from("&self");
            for p in &op.params {
                match p.direction {
                    ResolvedParamDirection::In => {
                        let ty = self.type_to_rust(&p.resolved_type);
                        params.push_str(&format!(", {}: {}", to_snake_case(&p.name), ty));
                    }
                    ResolvedParamDirection::Out => {
                        let ty = self.type_to_rust(&p.resolved_type);
                        params.push_str(&format!(", {}: &mut {}", to_snake_case(&p.name), ty));
                    }
                    ResolvedParamDirection::Inout => {
                        let ty = self.type_to_rust(&p.resolved_type);
                        params.push_str(&format!(", {}: &mut {}", to_snake_case(&p.name), ty));
                    }
                }
            }

            let ret = if let Some(ret_type) = &op.return_type {
                format!(" -> {}", self.type_to_rust(ret_type))
            } else {
                String::new()
            };

            self.line(&format!("fn {}({}){};", method_name, params, ret));
        }

        self.indent -= 1;
        self.line("}");
    }

    /// (7.9.2.1) Generate dispatcher that bridges Call/Return unions to the service trait.
    fn emit_service_dispatcher(&mut self, iface_name: &str, ops: &[ResolvedOperation]) {
        let call_type = format!("{}_Call", iface_name);
        let return_type = format!("{}_Return", iface_name);

        // Wrapper struct
        self.line(&format!("pub struct {0}Dispatcher<T: {0}> {{", iface_name));
        self.indent += 1;
        self.line("inner: T,");
        self.indent -= 1;
        self.line("}");
        self.line("");

        self.line(&format!("impl<T: {0}> {0}Dispatcher<T> {{", iface_name));
        self.indent += 1;
        self.line(&format!("pub fn new(inner: T) -> Self {{"));
        self.indent += 1;
        self.line("Self { inner }");
        self.indent -= 1;
        self.line("}");
        self.indent -= 1;
        self.line("}");
        self.line("");

        // RequestHandler impl
        self.line(&format!(
            "impl<T: {iface} + Send + 'static> RequestHandler<{call}, {ret}> for {iface}Dispatcher<T> {{",
            iface = iface_name, call = call_type, ret = return_type
        ));
        self.indent += 1;
        self.line(&format!(
            "fn handle_request(&self, request: &{}) -> {} {{",
            call_type, return_type
        ));
        self.indent += 1;
        self.line("match request {");
        self.indent += 1;

        for op in ops {
            let variant = naming::to_pascal_case(&op.name);
            let in_type = format!("{}_{}_In", iface_name, op.name);
            let out_type = format!("{}_{}_Out", iface_name, op.name);
            let result_type = format!("{}_{}_Result", iface_name, op.name);
            let method_name = to_snake_case(&op.name);

            // Build parameter destructure and call args
            let in_params: Vec<&ResolvedParam> = op
                .params
                .iter()
                .filter(|p| {
                    matches!(
                        p.direction,
                        ResolvedParamDirection::In | ResolvedParamDirection::Inout
                    )
                })
                .collect();

            let out_params: Vec<&ResolvedParam> = op
                .params
                .iter()
                .filter(|p| {
                    matches!(
                        p.direction,
                        ResolvedParamDirection::Out | ResolvedParamDirection::Inout
                    )
                })
                .collect();

            if in_params.is_empty() {
                self.line(&format!("{}::{}(_) => {{", call_type, variant));
            } else {
                let fields: Vec<String> = in_params.iter().map(|p| p.name.clone()).collect();
                self.line(&format!(
                    "{}::{}({} {{ {} }}) => {{",
                    call_type,
                    variant,
                    in_type,
                    fields.join(", ")
                ));
            }
            self.indent += 1;

            // Build the call arguments
            let mut call_args = Vec::new();
            for p in &op.params {
                match p.direction {
                    ResolvedParamDirection::In => {
                        // Check if the type is Copy-like (primitives) or needs clone
                        if Self::is_copy_type(&p.resolved_type) {
                            call_args.push(format!("*{}", p.name));
                        } else {
                            call_args.push(format!("{}.clone()", p.name));
                        }
                    }
                    ResolvedParamDirection::Out => {
                        // Out params: initialize default, pass as &mut
                        self.line(&format!(
                            "let mut {} = Default::default();",
                            to_snake_case(&p.name)
                        ));
                        call_args.push(format!("&mut {}", to_snake_case(&p.name)));
                    }
                    ResolvedParamDirection::Inout => {
                        // Inout: clone from In struct, pass as &mut
                        self.line(&format!("let mut {0}_mut = {0}.clone();", p.name));
                        call_args.push(format!("&mut {}_mut", p.name));
                    }
                }
            }

            let call_str = call_args.join(", ");

            if op.return_type.is_some() {
                self.line(&format!("let return_ = self.inner.{}({});", method_name, call_str));
            } else {
                self.line(&format!("self.inner.{}({});", method_name, call_str));
            }

            // Build Out struct
            let mut out_fields = Vec::new();
            for p in &op.params {
                match p.direction {
                    ResolvedParamDirection::Out => {
                        out_fields.push(format!("{0}: {0}", to_snake_case(&p.name)));
                    }
                    ResolvedParamDirection::Inout => {
                        out_fields.push(format!("{}: {}_mut", p.name, p.name));
                    }
                    _ => {}
                }
            }
            if op.return_type.is_some() {
                let return_name =
                    Self::resolve_return_name(&out_params.iter().map(|p| *p).collect::<Vec<_>>());
                out_fields.push(format!("{}: return_", return_name));
            }

            if out_fields.is_empty() {
                // void with no out params → dummy
                self.line(&format!(
                    "{}::{}({}::Result({} {{ dummy: UnusedMember }}))",
                    return_type, variant, result_type, out_type
                ));
            } else {
                self.line(&format!(
                    "{}::{}({}::Result({} {{ {} }}))",
                    return_type,
                    variant,
                    result_type,
                    out_type,
                    out_fields.join(", ")
                ));
            }

            self.indent -= 1;
            self.line("}");
        }

        // Default case for unknown operations
        self.line(&format!(
            "{}::UnknownOp(_) => {}::UnknownOp(UnknownOperation),",
            call_type, return_type
        ));

        self.indent -= 1;
        self.line("}");
        self.indent -= 1;
        self.line("}");
        self.indent -= 1;
        self.line("}");
    }

    /// (7.11.1.5.4) Generate typed client with per-operation methods.
    fn emit_client_struct(&mut self, iface_name: &str, ops: &[ResolvedOperation]) {
        let _request_type = format!("{}_Request", iface_name);
        let _reply_type = format!("{}_Reply", iface_name);
        let call_type = format!("{}_Call", iface_name);
        let return_type = format!("{}_Return", iface_name);
        let client_name = format!("{}Client", iface_name);

        // Struct definition
        self.line(&format!("pub struct {} {{", client_name));
        self.indent += 1;
        self.line(&format!("client: Client<{}, {}>,", call_type, return_type));
        self.indent -= 1;
        self.line("}");
        self.line("");

        // impl block
        self.line(&format!("impl {} {{", client_name));
        self.indent += 1;

        // Constructor
        self.line("pub fn new(params: ClientParams) -> DdsRpcResult<Self> {");
        self.indent += 1;
        self.line("let client = Client::new(params)?;");
        self.line("Ok(Self { client })");
        self.indent -= 1;
        self.line("}");

        // Per-operation sync methods
        for op in ops {
            self.line("");
            let method_name = to_snake_case(&op.name);
            let variant = naming::to_pascal_case(&op.name);
            let in_type = format!("{}_{}_In", iface_name, op.name);

            // Build method signature params
            let mut sig_params = String::from("&self");
            let in_params: Vec<&ResolvedParam> = op
                .params
                .iter()
                .filter(|p| {
                    matches!(
                        p.direction,
                        ResolvedParamDirection::In | ResolvedParamDirection::Inout
                    )
                })
                .collect();

            let out_params: Vec<&ResolvedParam> = op
                .params
                .iter()
                .filter(|p| {
                    matches!(
                        p.direction,
                        ResolvedParamDirection::Out | ResolvedParamDirection::Inout
                    )
                })
                .collect();

            for p in &in_params {
                let ty = self.type_to_rust(&p.resolved_type);
                sig_params.push_str(&format!(", {}: {}", to_snake_case(&p.name), ty));
            }
            sig_params.push_str(", timeout: Duration");

            // Return type
            let has_return = op.return_type.is_some();
            let has_out = !out_params.is_empty();

            let ret_type = if !has_return && !has_out {
                "()".to_string()
            } else {
                let mut parts = Vec::new();
                for p in &out_params {
                    parts.push(self.type_to_rust(&p.resolved_type));
                }
                if let Some(ret) = &op.return_type {
                    parts.push(self.type_to_rust(ret));
                }
                if parts.len() == 1 {
                    parts[0].clone()
                } else {
                    format!("({})", parts.join(", "))
                }
            };

            self.line(&format!(
                "pub fn {}({}) -> DdsRpcResult<{}> {{",
                method_name, sig_params, ret_type
            ));
            self.indent += 1;

            // Build In struct
            if in_params.is_empty() {
                self.line(&format!(
                    "let call = {}::{}({} {{ dummy: UnusedMember }});",
                    call_type, variant, in_type
                ));
            } else {
                let fields: Vec<String> =
                    in_params.iter().map(|p| to_snake_case(&p.name)).collect();
                self.line(&format!(
                    "let call = {}::{}({} {{ {} }});",
                    call_type,
                    variant,
                    in_type,
                    fields.join(", ")
                ));
            }

            // Send and receive
            self.line("let id = self.client.send_request(&call)?;");
            self.line("let reply = self.client.receive_reply(timeout)?;");
            self.line("let data = reply.data().map_err(|e| DdsRpcError::Dds(e.into()))?;");
            self.line("");

            // Check remote exception
            self.line("if data.header.remote_ex != RemoteExceptionCode::Ok {");
            self.indent += 1;
            self.line("return Err(DdsRpcError::Remote(data.header.remote_ex));");
            self.indent -= 1;
            self.line("}");
            self.line("");

            // Unpack Return union
            let result_type = format!("{}_{}_Result", iface_name, op.name);
            self.line(&format!("match &data.data {{"));
            self.indent += 1;
            self.line(&format!("{}::{}(result) => match result {{", return_type, variant));
            self.indent += 1;
            self.line(&format!("{}::Result(out) => {{", result_type));
            self.indent += 1;

            // Extract return value
            if !has_return && !has_out {
                self.line("Ok(())");
            } else {
                let mut fields = Vec::new();
                for p in &out_params {
                    fields.push(format!("out.{}.clone()", to_snake_case(&p.name)));
                }
                if has_return {
                    let return_name = Self::resolve_return_name(
                        &out_params.iter().map(|p| *p).collect::<Vec<_>>(),
                    );
                    fields.push(format!("out.{}.clone()", return_name));
                }
                if fields.len() == 1 {
                    self.line(&format!("Ok({})", fields[0]));
                } else {
                    self.line(&format!("Ok(({}))", fields.join(", ")));
                }
            }

            self.indent -= 1;
            self.line("}");
            // Exception variants
            self.line("_ => Err(DdsRpcError::Remote(RemoteExceptionCode::UnknownException)),");
            self.indent -= 1;
            self.line("}");
            // Wrong operation in return
            self.line("_ => Err(DdsRpcError::Remote(RemoteExceptionCode::UnknownOperation)),");
            self.indent -= 1;
            self.line("}");

            self.indent -= 1;
            self.line("}");
        }

        self.indent -= 1;
        self.line("}");
    }

    fn is_copy_type(ty: &ResolvedType) -> bool {
        matches!(
            ty,
            ResolvedType::Bool
                | ResolvedType::U8
                | ResolvedType::I8
                | ResolvedType::I16
                | ResolvedType::U16
                | ResolvedType::I32
                | ResolvedType::U32
                | ResolvedType::I64
                | ResolvedType::U64
                | ResolvedType::F32
                | ResolvedType::F64
                | ResolvedType::Char
                | ResolvedType::WChar
        )
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

    /// Extract the body (between `{` and `}`) of a named struct/enum from generated code.
    fn extract_body<'a>(code: &'a str, header: &str) -> &'a str {
        let start =
            code.find(header).unwrap_or_else(|| panic!("'{}' not found in:\n{}", header, code));
        let body_start = start + header.len();
        let end = body_start + code[body_start..].find('}').unwrap();
        &code[body_start..end]
    }

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
        let h = rpc_hash("");
        let expected = i32::from_le_bytes([0xd4, 0x1d, 0x8c, 0xd9]);
        assert_eq!(h, expected);
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
        let body = extract_body(&code, "pub struct RobotControl_getSpeed_In {");

        assert!(body.contains("pub dummy: UnusedMember,"));
    }

    #[test]
    fn test_in_struct_filters_out_params() {
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
        let body = extract_body(&code, "pub struct Foo_bar_In {");

        assert!(body.contains("pub a: i32,"));
        assert!(!body.contains("pub b: i32,"));
        assert!(body.contains("pub c: i32,"));
    }

    #[test]
    fn test_in_struct_param_order() {
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
        let body = extract_body(&code, "pub struct Calc_compute_In {");

        let x_pos = body.find("pub x: i32,").unwrap();
        let y_pos = body.find("pub y: f64,").unwrap();
        let z_pos = body.find("pub z: String,").unwrap();
        assert!(x_pos < y_pos && y_pos < z_pos);
    }

    #[test]
    fn test_out_struct_void_no_out_params() {
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
        let body = extract_body(&code, "pub struct Foo_fire_Out {");

        assert!(body.contains("pub dummy: UnusedMember,"));
    }

    #[test]
    fn test_out_struct_with_return() {
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
        let body = extract_body(&code, "pub struct Foo_getSpeed_Out {");

        assert!(body.contains("pub return_: f32,"));
    }

    #[test]
    fn test_out_struct_with_out_params_and_return() {
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
        let body = extract_body(&code, "pub struct Foo_compute_Out {");

        let y_pos = body.find("pub y: f64,").unwrap();
        let z_pos = body.find("pub z: String,").unwrap();
        let ret_pos = body.find("pub return_: i32,").unwrap();
        assert!(y_pos < z_pos && z_pos < ret_pos);
    }

    #[test]
    fn test_out_struct_return_name_collision() {
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

        assert!(code.contains("pub return_: i32,"));
        assert!(code.contains("pub return_1: i32,"));
    }

    #[test]
    fn test_out_struct_only_out_params_no_return() {
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
        let body = extract_body(&code, "pub struct Foo_getPosition_Out {");

        assert!(body.contains("pub x: f32,"));
        assert!(body.contains("pub y: f32,"));
        assert!(!body.contains("dummy"));
        assert!(!body.contains("return_"));
    }

    #[test]
    fn test_result_union_no_exceptions() {
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

        assert!(code.contains("pub enum Foo_getSpeed_Result {"));
        assert!(code.contains("Result(Foo_getSpeed_Out) = 0,"));
    }

    #[test]
    fn test_result_union_with_exceptions() {
        let defs = parse_idl(
            r#"
            exception TooFast {
                float speed;
            };
            interface RobotControl {
                void setSpeed(in float speed) raises (TooFast);
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        let hash_val = rpc_hash("TooFast");
        assert!(code.contains(&format!("pub const TOO_FAST_EX_HASH: i32 = {};", hash_val)));
        assert!(code.contains("pub enum RobotControl_setSpeed_Result {"));
        assert!(code.contains("Result(RobotControl_setSpeed_Out) = 0,"));
        assert!(code.contains("TooFast_ex(TooFast) = TOO_FAST_EX_HASH,"));
    }

    #[test]
    fn test_call_union() {
        let defs = parse_idl(
            r#"
            interface RobotControl {
                void setSpeed(in float speed);
                float getSpeed();
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        // Hash constants
        let set_hash = rpc_hash("setSpeed");
        let get_hash = rpc_hash("getSpeed");
        assert!(
            code.contains(&format!("pub const ROBOT_CONTROL_SET_SPEED_HASH: i32 = {};", set_hash))
        );
        assert!(
            code.contains(&format!("pub const ROBOT_CONTROL_GET_SPEED_HASH: i32 = {};", get_hash))
        );

        // Call union
        let body = extract_body(&code, "pub enum RobotControl_Call {");
        assert!(body.contains("UnknownOp(UnknownOperation) = -1,"));
        assert!(body.contains("SetSpeed(RobotControl_setSpeed_In) = ROBOT_CONTROL_SET_SPEED_HASH,"));
        assert!(body.contains("GetSpeed(RobotControl_getSpeed_In) = ROBOT_CONTROL_GET_SPEED_HASH,"));
    }

    #[test]
    fn test_return_union() {
        let defs = parse_idl(
            r#"
            interface RobotControl {
                void setSpeed(in float speed);
                float getSpeed();
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        let body = extract_body(&code, "pub enum RobotControl_Return {");
        assert!(body.contains("UnknownOp(UnknownOperation) = -1,"));
        assert!(
            body.contains("SetSpeed(RobotControl_setSpeed_Result) = ROBOT_CONTROL_SET_SPEED_HASH,")
        );
        assert!(
            body.contains("GetSpeed(RobotControl_getSpeed_Result) = ROBOT_CONTROL_GET_SPEED_HASH,")
        );
    }

    #[test]
    fn test_request_struct() {
        let defs = parse_idl(
            r#"
            interface Foo {
                void bar();
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        let body = extract_body(&code, "pub struct Foo_Request {");
        assert!(body.contains("pub header: RequestHeader,"));
        assert!(body.contains("pub data: Foo_Call,"));
    }

    #[test]
    fn test_reply_struct() {
        let defs = parse_idl(
            r#"
            interface Foo {
                void bar();
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        let body = extract_body(&code, "pub struct Foo_Reply {");
        assert!(body.contains("pub header: ReplyHeader,"));
        assert!(body.contains("pub data: Foo_Return,"));
    }

    #[test]
    fn test_inheritance_only_own_operations() {
        // (7.5.1.1.8) derived interface includes only its own operations
        let defs = parse_idl(
            r#"
            interface Adder {
                long add(in long a, in long b);
            };
            interface Calculator : Adder {
                void on();
                void off();
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        // Adder gets its own types
        assert!(code.contains("pub struct Adder_add_In {"));
        assert!(code.contains("pub enum Adder_Call {"));
        assert!(code.contains("pub struct Adder_Request {"));

        // Calculator gets only on/off, NOT add
        assert!(code.contains("pub struct Calculator_on_In {"));
        assert!(code.contains("pub struct Calculator_off_In {"));
        assert!(!code.contains("pub struct Calculator_add_In {"));

        let calc_call = extract_body(&code, "pub enum Calculator_Call {");
        assert!(calc_call.contains("On("));
        assert!(calc_call.contains("Off("));
        assert!(!calc_call.contains("Add("));
    }

    #[test]
    fn test_attribute_readwrite() {
        let defs = parse_idl(
            r#"
            interface Sensor {
                attribute float temperature;
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        // getter: get_attribute_temperature() → returns float, no in params
        let getter_in = extract_body(&code, "pub struct Sensor_get_attribute_temperature_In {");
        assert!(getter_in.contains("pub dummy: UnusedMember,"));
        let getter_out = extract_body(&code, "pub struct Sensor_get_attribute_temperature_Out {");
        assert!(getter_out.contains("pub return_: f32,"));

        // setter: set_attribute_temperature(in float temperature) → void
        let setter_in = extract_body(&code, "pub struct Sensor_set_attribute_temperature_In {");
        assert!(setter_in.contains("pub temperature: f32,"));
        let setter_out = extract_body(&code, "pub struct Sensor_set_attribute_temperature_Out {");
        assert!(setter_out.contains("pub dummy: UnusedMember,"));
    }

    #[test]
    fn test_attribute_readonly() {
        // readonly → getter only, no setter
        let defs = parse_idl(
            r#"
            interface Sensor {
                readonly attribute float temperature;
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        assert!(code.contains("pub struct Sensor_get_attribute_temperature_In {"));
        assert!(!code.contains("set_attribute_temperature"));
    }

    #[test]
    #[should_panic(expected = "name collision")]
    fn test_attribute_name_collision() {
        let defs = parse_idl(
            r#"
            interface Bad {
                attribute float speed;
                void get_attribute_speed();
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let _ = generate(&model, &RpcOptions::default());
    }

    /// Spec 7.5.1.1.4 ~ 7.5.1.1.7 RobotControl example
    #[test]
    fn test_spec_robot_control() {
        let defs = parse_idl(
            r#"
            enum Command { CYCLIC, POSITION };
            exception TooFast { float speed; };
            interface RobotControl {
                void command(in Command com);
                void setSpeed(in float speed) raises (TooFast);
                float getSpeed();
                long getStatus();
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        // In structs (7.5.1.1.4)
        let cmd_in = extract_body(&code, "pub struct RobotControl_command_In {");
        assert!(cmd_in.contains("pub com: Command,"));

        let set_in = extract_body(&code, "pub struct RobotControl_setSpeed_In {");
        assert!(set_in.contains("pub speed: f32,"));

        let get_in = extract_body(&code, "pub struct RobotControl_getSpeed_In {");
        assert!(get_in.contains("pub dummy: UnusedMember,"));

        let status_in = extract_body(&code, "pub struct RobotControl_getStatus_In {");
        assert!(status_in.contains("pub dummy: UnusedMember,"));

        // Out structs (7.5.1.1.5)
        let cmd_out = extract_body(&code, "pub struct RobotControl_command_Out {");
        assert!(cmd_out.contains("pub dummy: UnusedMember,")); // void, no out params

        let set_out = extract_body(&code, "pub struct RobotControl_setSpeed_Out {");
        assert!(set_out.contains("pub dummy: UnusedMember,")); // void, no out params

        let get_out = extract_body(&code, "pub struct RobotControl_getSpeed_Out {");
        assert!(get_out.contains("pub return_: f32,")); // float return

        let status_out = extract_body(&code, "pub struct RobotControl_getStatus_Out {");
        assert!(status_out.contains("pub return_: i32,")); // long return

        // Result unions (7.5.1.1.5)
        // command: no exceptions
        let cmd_result = extract_body(&code, "pub enum RobotControl_command_Result {");
        assert!(cmd_result.contains("Result(RobotControl_command_Out) = 0,"));

        // setSpeed: raises TooFast
        let hash_val = rpc_hash("TooFast");
        assert!(code.contains(&format!("pub const TOO_FAST_EX_HASH: i32 = {};", hash_val)));
        let set_result = extract_body(&code, "pub enum RobotControl_setSpeed_Result {");
        assert!(set_result.contains("Result(RobotControl_setSpeed_Out) = 0,"));
        assert!(set_result.contains("TooFast_ex(TooFast) = TOO_FAST_EX_HASH,"));

        // Hash constants (7.5.1.1.6 rule 4)
        assert!(code.contains(&format!(
            "pub const ROBOT_CONTROL_COMMAND_HASH: i32 = {};",
            rpc_hash("command")
        )));
        assert!(code.contains(&format!(
            "pub const ROBOT_CONTROL_SET_SPEED_HASH: i32 = {};",
            rpc_hash("setSpeed")
        )));
        assert!(code.contains(&format!(
            "pub const ROBOT_CONTROL_GET_SPEED_HASH: i32 = {};",
            rpc_hash("getSpeed")
        )));
        assert!(code.contains(&format!(
            "pub const ROBOT_CONTROL_GET_STATUS_HASH: i32 = {};",
            rpc_hash("getStatus")
        )));

        // Call union (7.5.1.1.6)
        let call = extract_body(&code, "pub enum RobotControl_Call {");
        assert!(call.contains("UnknownOp(UnknownOperation) = -1,"));
        assert!(call.contains("Command(RobotControl_command_In) = ROBOT_CONTROL_COMMAND_HASH,"));
        assert!(call.contains("SetSpeed(RobotControl_setSpeed_In) = ROBOT_CONTROL_SET_SPEED_HASH,"));
        assert!(call.contains("GetSpeed(RobotControl_getSpeed_In) = ROBOT_CONTROL_GET_SPEED_HASH,"));
        assert!(
            call.contains("GetStatus(RobotControl_getStatus_In) = ROBOT_CONTROL_GET_STATUS_HASH,")
        );

        // Return union (7.5.1.1.7)
        let ret = extract_body(&code, "pub enum RobotControl_Return {");
        assert!(ret.contains("UnknownOp(UnknownOperation) = -1,"));
        assert!(ret.contains("Command(RobotControl_command_Result) = ROBOT_CONTROL_COMMAND_HASH,"));
        assert!(
            ret.contains("SetSpeed(RobotControl_setSpeed_Result) = ROBOT_CONTROL_SET_SPEED_HASH,")
        );
        assert!(
            ret.contains("GetSpeed(RobotControl_getSpeed_Result) = ROBOT_CONTROL_GET_SPEED_HASH,")
        );
        assert!(ret
            .contains("GetStatus(RobotControl_getStatus_Result) = ROBOT_CONTROL_GET_STATUS_HASH,"));

        // Request / Reply (7.5.1.1.6, 7.5.1.1.7)
        let req = extract_body(&code, "pub struct RobotControl_Request {");
        assert!(req.contains("pub header: RequestHeader,"));
        assert!(req.contains("pub data: RobotControl_Call,"));

        let reply = extract_body(&code, "pub struct RobotControl_Reply {");
        assert!(reply.contains("pub header: ReplyHeader,"));
        assert!(reply.contains("pub data: RobotControl_Return,"));
    }

    /// Spec 7.5.1.1.8 Calculator inheritance example (Adder + Subtractor + Calculator)
    #[test]
    fn test_spec_calculator_inheritance() {
        let defs = parse_idl(
            r#"
            interface Adder {
                long add(in long a, in long b);
            };
            interface Subtractor {
                long sub(in long a, in long b);
            };
            interface Calculator : Adder, Subtractor {
                void on();
                void off();
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        // Adder: only "add"
        let adder_in = extract_body(&code, "pub struct Adder_add_In {");
        assert!(adder_in.contains("pub a: i32,"));
        assert!(adder_in.contains("pub b: i32,"));

        let adder_out = extract_body(&code, "pub struct Adder_add_Out {");
        assert!(adder_out.contains("pub return_: i32,"));

        let adder_call = extract_body(&code, "pub enum Adder_Call {");
        assert!(adder_call.contains("UnknownOp(UnknownOperation) = -1,"));
        assert!(adder_call.contains("Add(Adder_add_In)"));
        assert!(!adder_call.contains("Sub(")); // not inherited

        let adder_req = extract_body(&code, "pub struct Adder_Request {");
        assert!(adder_req.contains("pub data: Adder_Call,"));

        let adder_reply = extract_body(&code, "pub struct Adder_Reply {");
        assert!(adder_reply.contains("pub data: Adder_Return,"));

        // Subtractor: only "sub"
        let sub_in = extract_body(&code, "pub struct Subtractor_sub_In {");
        assert!(sub_in.contains("pub a: i32,"));

        let sub_out = extract_body(&code, "pub struct Subtractor_sub_Out {");
        assert!(sub_out.contains("pub return_: i32,"));

        let sub_call = extract_body(&code, "pub enum Subtractor_Call {");
        assert!(sub_call.contains("Sub(Subtractor_sub_In)"));
        assert!(!sub_call.contains("Add("));

        assert!(code.contains("pub struct Subtractor_Request {"));
        assert!(code.contains("pub struct Subtractor_Reply {"));

        // Calculator: only "on" and "off", NOT add/sub
        let calc_on_in = extract_body(&code, "pub struct Calculator_on_In {");
        assert!(calc_on_in.contains("pub dummy: UnusedMember,")); // void, no params

        let calc_off_in = extract_body(&code, "pub struct Calculator_off_In {");
        assert!(calc_off_in.contains("pub dummy: UnusedMember,"));

        let calc_on_out = extract_body(&code, "pub struct Calculator_on_Out {");
        assert!(calc_on_out.contains("pub dummy: UnusedMember,"));

        let calc_call = extract_body(&code, "pub enum Calculator_Call {");
        assert!(calc_call.contains("UnknownOp(UnknownOperation) = -1,"));
        assert!(calc_call.contains("On(Calculator_on_In)"));
        assert!(calc_call.contains("Off(Calculator_off_In)"));
        assert!(!calc_call.contains("Add(")); // no inherited ops
        assert!(!calc_call.contains("Sub("));

        let calc_return = extract_body(&code, "pub enum Calculator_Return {");
        assert!(calc_return.contains("On(Calculator_on_Result)"));
        assert!(calc_return.contains("Off(Calculator_off_Result)"));
        assert!(!calc_return.contains("Add("));
        assert!(!calc_return.contains("Sub("));

        // Hash constants
        assert!(code.contains(&format!("pub const CALCULATOR_ON_HASH: i32 = {};", rpc_hash("on"))));
        assert!(
            code.contains(&format!("pub const CALCULATOR_OFF_HASH: i32 = {};", rpc_hash("off")))
        );

        assert!(code.contains("pub struct Calculator_Request {"));
        assert!(code.contains("pub struct Calculator_Reply {"));
    }

    #[test]
    fn test_service_trait() {
        let defs = parse_idl(
            r#"
            interface RobotControl {
                void setSpeed(in float speed);
                float getSpeed();
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        assert!(code.contains("pub trait RobotControl {"));
        assert!(code.contains("fn set_speed(&self, speed: f32);"));
        assert!(code.contains("fn get_speed(&self) -> f32;"));
    }

    #[test]
    fn test_service_trait_out_params() {
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

        assert!(code.contains("fn get_position(&self, x: &mut f32, y: &mut f32);"));
    }

    #[test]
    fn test_service_dispatcher() {
        let defs = parse_idl(
            r#"
            interface RobotControl {
                void setSpeed(in float speed);
                float getSpeed();
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        assert!(code.contains("pub struct RobotControlDispatcher<T: RobotControl>"));
        assert!(code.contains("impl<T: RobotControl + Send + 'static> RequestHandler<RobotControl_Call, RobotControl_Return> for RobotControlDispatcher<T>"));
        assert!(code.contains(
            "fn handle_request(&self, request: &RobotControl_Call) -> RobotControl_Return"
        ));
    }

    #[test]
    fn test_service_dispatcher_void_dispatch() {
        let defs = parse_idl(
            r#"
            interface Foo {
                void fire();
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        // void operation → dispatches, constructs Out with dummy
        assert!(code.contains("self.inner.fire();"));
        assert!(code.contains("dummy: UnusedMember"));
    }

    #[test]
    fn test_service_dispatcher_return_value() {
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

        assert!(code.contains("let return_ = self.inner.get_speed();"));
        assert!(code.contains("return_: return_"));
    }

    #[test]
    fn test_client_struct() {
        let defs = parse_idl(
            r#"
            interface RobotControl {
                void setSpeed(in float speed);
                float getSpeed();
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        assert!(code.contains("pub struct RobotControlClient {"));
        assert!(code.contains("client: Client<RobotControl_Call, RobotControl_Return>,"));
        assert!(code.contains("pub fn new(params: ClientParams) -> DdsRpcResult<Self>"));
        assert!(code.contains(
            "pub fn set_speed(&self, speed: f32, timeout: Duration) -> DdsRpcResult<()>"
        ));
        assert!(code.contains("pub fn get_speed(&self, timeout: Duration) -> DdsRpcResult<f32>"));
    }

    #[test]
    fn test_client_out_params() {
        let defs = parse_idl(
            r#"
            interface Foo {
                long compute(in long x, out double y);
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        // out param + return → tuple return
        assert!(code.contains(
            "pub fn compute(&self, x: i32, timeout: Duration) -> DdsRpcResult<(f64, i32)>"
        ));
    }

    /// Full RobotControl example: verify trait + dispatcher + client are all generated
    #[test]
    fn test_full_robot_control_function_call() {
        let defs = parse_idl(
            r#"
            enum Command { CYCLIC, POSITION };
            exception TooFast { float speed; };
            interface RobotControl {
                void command(in Command com);
                void setSpeed(in float speed) raises (TooFast);
                float getSpeed();
                long getStatus();
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        // Service trait
        assert!(code.contains("pub trait RobotControl {"));
        assert!(code.contains("fn command(&self, com: Command);"));
        assert!(code.contains("fn set_speed(&self, speed: f32);"));
        assert!(code.contains("fn get_speed(&self) -> f32;"));
        assert!(code.contains("fn get_status(&self) -> i32;"));

        // Dispatcher
        assert!(code.contains("pub struct RobotControlDispatcher<T: RobotControl>"));
        assert!(code.contains("RobotControl_Call::Command("));
        assert!(code.contains("RobotControl_Call::SetSpeed("));
        assert!(code.contains("RobotControl_Call::GetSpeed("));
        assert!(code.contains("RobotControl_Call::GetStatus("));

        // Client
        assert!(code.contains("pub struct RobotControlClient {"));
        assert!(code.contains(
            "pub fn command(&self, com: Command, timeout: Duration) -> DdsRpcResult<()>"
        ));
        assert!(code.contains(
            "pub fn set_speed(&self, speed: f32, timeout: Duration) -> DdsRpcResult<()>"
        ));
        assert!(code.contains("pub fn get_speed(&self, timeout: Duration) -> DdsRpcResult<f32>"));
        assert!(code.contains("pub fn get_status(&self, timeout: Duration) -> DdsRpcResult<i32>"));
    }
}
