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

    gen.line(&format!(
        "use {}_rpc::types::{{UnusedMember, UnknownOperation, UnknownException, RequestHeader, ReplyHeader, RemoteExceptionCode}};",
        opts.crate_path
    ));
    gen.line(&format!(
        "use {}_rpc::client::{{Client, ClientParams}};",
        opts.crate_path
    ));
    gen.line(&format!(
        "use {}_rpc::service::{{Service, ServiceParams, RequestHandler}};",
        opts.crate_path
    ));
    gen.line(&format!(
        "use {}_rpc::error::{{DdsRpcError, DdsRpcResult}};",
        opts.crate_path
    ));
    gen.line(&format!(
        "use {}_rpc::types::SampleIdentity;",
        opts.crate_path
    ));
    gen.line(&format!(
        "use {}_rpc::requester::Future;",
        opts.crate_path
    ));
    gen.line(&format!(
        "use {}_rpc::entity::{{RpcEntity, ServiceProxy}};",
        opts.crate_path
    ));
    gen.line(&format!(
        "use {}_rpc::client::ClientEndpoint;",
        opts.crate_path
    ));
    gen.line(&format!(
        "use {}_rpc::server::Dispatchable;",
        opts.crate_path
    ));
    gen.line(&format!(
        "use {}_rpc::types::InstanceName;",
        opts.crate_path
    ));
    gen.line("use std::time::Duration;");
    gen.line(&format!(
        "use {}::serialize::cdr::serializer::primitive::PrimitiveSerialize;",
        opts.crate_path
    ));
    gen.line("");

    for exc in &model.exceptions {
        gen.emit_exception(exc);
    }

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
    /// Emit exception as a `#[derive(DdsType, Clone)]` struct.
    fn emit_exception(&mut self, exc: &ResolvedException) {
        let rust_name = naming::to_pascal_case(&exc.name);
        self.line("#[derive(DdsType, Debug, Clone)]");
        self.line(&format!("#[dds_type(crate_path = \"{}\", no_additional_derives)]", self.opts.crate_path));
        self.line(&format!("pub struct {} {{", rust_name));
        self.indent += 1;
        for m in &exc.members {
            let field_name = naming::to_snake_case(&m.name);
            let field_type = self.type_to_rust(&m.resolved_type);
            self.line(&format!("pub {}: {},", field_name, field_type));
        }
        self.indent -= 1;
        self.line("}");
        self.line("");
    }

    fn emit_interface(&mut self, iface: &ResolvedInterface) {
        // (7.5.1.1.3) Expand attributes to implied operations
        let mut all_ops = iface.operations.clone();
        for attr in &iface.attributes {
            Self::validate_attribute_names(attr, &iface.operations);
            all_ops.extend(Self::expand_attribute(attr));
        }

        // (rule 4) Emit unique exception hash constants for the entire interface
        let mut emitted_exc_hashes = std::collections::HashSet::new();
        for op in &all_ops {
            for exc_name in &op.raises {
                if emitted_exc_hashes.insert(exc_name.clone()) {
                    let simple = exc_name.rsplit("::").next().unwrap_or(exc_name);
                    let hash_const_name = format!("{}_EX_HASH", naming::to_screaming_snake(simple));
                    let hash_value = rpc_hash(exc_name);
                    self.line(&format!("pub const {}: i32 = {};", hash_const_name, hash_value));
                }
            }
        }
        if !emitted_exc_hashes.is_empty() {
            self.line("");
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

        // Per-operation service error enums (Step 4: raises 2+ only)
        for op in &all_ops {
            self.emit_service_error_enum(&iface.name, op);
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

        // Per-operation Future wrappers (Step 9)
        for op in &all_ops {
            self.emit_operation_future(&iface.name, op);
            self.line("");
        }

        // Function-call style (7.11.1.5)
        self.emit_service_trait(&iface.name, &all_ops);
        self.line("");
        self.emit_async_trait(&iface.name, &all_ops);
        self.line("");
        self.emit_service_dispatcher(&iface.name, &all_ops);
        self.line("");
        self.emit_service_wrapper(&iface.name, &iface.qualified_name);
        self.line("");
        self.emit_client_struct(&iface.name, &iface.qualified_name, &all_ops);
        self.line("");
        self.emit_client_async_impl(&iface.name, &all_ops);
        self.line("");
        self.emit_client_endpoint_impl(&iface.name);
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
            .filter(|p| matches!(p.direction, ResolvedParamDirection::In | ResolvedParamDirection::Inout))
            .collect();

        self.line("#[derive(DdsType, Debug, Clone)]");
        self.line(&format!("#[dds_type(crate_path = \"{}\", no_additional_derives)]", self.opts.crate_path));
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

        self.line("#[derive(DdsType, Debug, Clone)]");
        self.line(&format!("#[dds_type(crate_path = \"{}\", no_additional_derives)]", self.opts.crate_path));
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

        self.line("#[derive(DdsType, Debug, Clone)]");
        self.line(&format!("#[dds_type(crate_path = \"{}\", no_additional_derives)]", self.opts.crate_path));
        self.line("#[repr(i32)]");
        self.line(&format!("pub enum {} {{", union_name));
        self.indent += 1;

        self.line(&format!("Result({}) = 0,", out_type));

        for exc_name in &op.raises {
            let simple = exc_name.rsplit("::").next().unwrap_or(exc_name);
            let hash_const_name = format!("{}_EX_HASH", naming::to_screaming_snake(simple));
            let member_name = format!("{}_ex", naming::to_pascal_case(simple));
            let exc_type = naming::to_pascal_case(simple);
            self.line(&format!(
                "{}({}) = {},",
                member_name, exc_type, hash_const_name
            ));
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

        self.line("#[derive(DdsType, Debug, Clone)]");
        self.line(&format!("#[dds_type(crate_path = \"{}\", no_additional_derives)]", self.opts.crate_path));
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

        self.line("#[derive(DdsType, Debug, Clone)]");
        self.line(&format!("#[dds_type(crate_path = \"{}\", no_additional_derives)]", self.opts.crate_path));
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

        self.line("#[derive(DdsType, Debug, Clone)]");
        self.line(&format!("#[dds_type(crate_path = \"{}\", no_additional_derives)]", self.opts.crate_path));
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

        self.line("#[derive(DdsType, Debug, Clone)]");
        self.line(&format!("#[dds_type(crate_path = \"{}\", no_additional_derives)]", self.opts.crate_path));
        self.line(&format!("pub struct {} {{", struct_name));
        self.indent += 1;
        self.line("pub header: ReplyHeader,");
        self.line(&format!("pub data: {},", return_type));
        self.indent -= 1;
        self.line("}");
    }

    /// (7.11.1.5) Generate service trait.
    /// Users implement this trait to provide the service logic.
    /// raises clause → Result<T, E> return type.
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

            let inner_ret = if let Some(ret_type) = &op.return_type {
                self.type_to_rust(ret_type)
            } else {
                "()".to_string()
            };

            let ret = if op.raises.is_empty() {
                if inner_ret == "()" { String::new() } else { format!(" -> {}", inner_ret) }
            } else {
                let err_type = Self::error_type_for_op(iface_name, op);
                format!(" -> Result<{}, {}>", inner_ret, err_type)
            };

            self.line(&format!("fn {}({}){};", method_name, params, ret));
        }

        self.indent -= 1;
        self.line("}");
    }

    /// (7.9.2.1) Generate dispatcher that bridges Call/Return unions to the service trait.
    /// Returns `(TRep, RemoteExceptionCode)` to support user exceptions and interface evolution.
    fn emit_service_dispatcher(&mut self, iface_name: &str, ops: &[ResolvedOperation]) {
        let call_type = format!("{}_Call", iface_name);
        let return_type = format!("{}_Return", iface_name);

        // Wrapper struct
        self.line(&format!(
            "pub struct {0}Dispatcher<T: {0}> {{", iface_name
        ));
        self.indent += 1;
        self.line("inner: T,");
        self.indent -= 1;
        self.line("}");
        self.line("");

        self.line(&format!(
            "impl<T: {0}> {0}Dispatcher<T> {{", iface_name
        ));
        self.indent += 1;
        self.line("pub fn new(inner: T) -> Self {");
        self.indent += 1;
        self.line("Self { inner }");
        self.indent -= 1;
        self.line("}");
        self.indent -= 1;
        self.line("}");
        self.line("");

        // RequestHandler impl — returns (TRep, RemoteExceptionCode)
        self.line(&format!(
            "impl<T: {iface} + Send + 'static> RequestHandler<{call}, {ret}> for {iface}Dispatcher<T> {{",
            iface = iface_name, call = call_type, ret = return_type
        ));
        self.indent += 1;
        self.line(&format!(
            "fn handle_request(&self, request: &{}) -> ({}, RemoteExceptionCode) {{",
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

            let in_params: Vec<&ResolvedParam> = op.params.iter()
                .filter(|p| matches!(p.direction, ResolvedParamDirection::In | ResolvedParamDirection::Inout))
                .collect();

            let out_params: Vec<&ResolvedParam> = op.params.iter()
                .filter(|p| matches!(p.direction, ResolvedParamDirection::Out | ResolvedParamDirection::Inout))
                .collect();

            if in_params.is_empty() {
                self.line(&format!("{}::{}(_) => {{", call_type, variant));
            } else {
                let fields: Vec<String> = in_params.iter()
                    .map(|p| p.name.clone())
                    .collect();
                self.line(&format!("{}::{}({} {{ {} }}) => {{",
                    call_type, variant, in_type,
                    fields.join(", ")));
            }
            self.indent += 1;

            // Build the call arguments + out param setup
            let mut call_args = Vec::new();
            for p in &op.params {
                match p.direction {
                    ResolvedParamDirection::In => {
                        if Self::is_copy_type(&p.resolved_type) {
                            call_args.push(format!("*{}", p.name));
                        } else {
                            call_args.push(format!("{}.clone()", p.name));
                        }
                    }
                    ResolvedParamDirection::Out => {
                        self.line(&format!("let mut {} = Default::default();",
                            to_snake_case(&p.name)));
                        call_args.push(format!("&mut {}", to_snake_case(&p.name)));
                    }
                    ResolvedParamDirection::Inout => {
                        self.line(&format!("let mut {0}_mut = {0}.clone();", p.name));
                        call_args.push(format!("&mut {}_mut", p.name));
                    }
                }
            }

            let call_str = call_args.join(", ");

            // Build Out struct fields
            let out_fields = Self::build_out_fields(op, &out_params);

            let ok_expr = if out_fields.is_empty() {
                format!("{}::{}({}::Result({} {{ dummy: UnusedMember {{}} }}))",
                    return_type, variant, result_type, out_type)
            } else {
                format!("{}::{}({}::Result({} {{ {} }}))",
                    return_type, variant, result_type, out_type,
                    out_fields.join(", "))
            };

            if op.raises.is_empty() {
                // No raises: direct call, wrap in tuple with Ok
                if op.return_type.is_some() {
                    self.line(&format!("let return_ = self.inner.{}({});", method_name, call_str));
                } else {
                    self.line(&format!("self.inner.{}({});", method_name, call_str));
                }
                self.line(&format!("({}, RemoteExceptionCode::Ok)", ok_expr));
            } else {
                // raises: match on Result
                self.line(&format!("match self.inner.{}({}) {{", method_name, call_str));
                self.indent += 1;

                // Ok branch
                if op.return_type.is_some() {
                    self.line(&format!("Ok(return_) => ({}, RemoteExceptionCode::Ok),", ok_expr));
                } else {
                    self.line(&format!("Ok(()) => ({}, RemoteExceptionCode::Ok),", ok_expr));
                }

                // Err branch
                if op.raises.len() == 1 {
                    // Single exception: Err(ex) directly
                    let exc_name = &op.raises[0];
                    let simple = exc_name.rsplit("::").next().unwrap_or(exc_name);
                    let member_name = format!("{}_ex", naming::to_pascal_case(simple));
                    self.line(&format!(
                        "Err(ex) => ({}::{}({}::{}(ex)), RemoteExceptionCode::Ok),",
                        return_type, variant, result_type, member_name
                    ));
                } else {
                    // Multiple exceptions: error enum match
                    let error_enum = format!("{}_{}_Error", iface_name, op.name);
                    self.line("Err(err) => match err {");
                    self.indent += 1;
                    for exc_name in &op.raises {
                        let simple = exc_name.rsplit("::").next().unwrap_or(exc_name);
                        let pascal = naming::to_pascal_case(simple);
                        let member_name = format!("{}_ex", pascal);
                        self.line(&format!(
                            "{}::{}(ex) => ({}::{}({}::{}(ex)), RemoteExceptionCode::Ok),",
                            error_enum, pascal,
                            return_type, variant, result_type, member_name
                        ));
                    }
                    self.indent -= 1;
                    self.line("},");
                }

                self.indent -= 1;
                self.line("}");
            }

            self.indent -= 1;
            self.line("}");
        }

        // (7.7.1) UnknownOp → Unsupported
        self.line(&format!(
            "{}::UnknownOp(_) => ({}::UnknownOp(UnknownOperation {{}}), RemoteExceptionCode::Unsupported),",
            call_type, return_type));

        self.indent -= 1;
        self.line("}");
        self.indent -= 1;
        self.line("}");
        self.indent -= 1;
        self.line("}");
    }

    /// Generate typed service wrapper that auto-injects interface_name into ServiceParams.
    fn emit_service_wrapper(&mut self, iface_name: &str, qualified_name: &str) {
        let call_type = format!("{}_Call", iface_name);
        let return_type = format!("{}_Return", iface_name);
        let service_name = format!("{}Service", iface_name);
        let dispatcher_type = format!("{}Dispatcher", iface_name);

        // Struct
        self.line(&format!(
            "pub struct {}<T: {} + Send + 'static> {{",
            service_name, iface_name
        ));
        self.indent += 1;
        self.line(&format!(
            "inner: Service<{}, {}, {}<T>>,",
            call_type, return_type, dispatcher_type
        ));
        self.indent -= 1;
        self.line("}");
        self.line("");

        // impl
        self.line(&format!(
            "impl<T: {} + Send + 'static> {}<T> {{",
            iface_name, service_name
        ));
        self.indent += 1;

        self.line("pub fn new(params: ServiceParams, handler: T) -> DdsRpcResult<Self> {");
        self.indent += 1;
        self.line(&format!(
            "let params = params.interface_name(\"{}\");",
            qualified_name
        ));
        self.line(&format!(
            "let inner = Service::new(params, {}::new(handler))?;",
            dispatcher_type
        ));
        self.line("Ok(Self { inner })");
        self.indent -= 1;
        self.line("}");

        self.indent -= 1;
        self.line("}");
        self.line("");

        // Dispatchable
        self.line(&format!(
            "impl<T: {} + Send + Sync + 'static> Dispatchable for {}<T> {{",
            iface_name, service_name
        ));
        self.indent += 1;
        self.line("fn try_dispatch_one(&self) -> DdsRpcResult<bool> {");
        self.indent += 1;
        self.line("self.inner.try_dispatch_one()");
        self.indent -= 1;
        self.line("}");
        self.indent -= 1;
        self.line("}");
        self.line("");

        // RpcEntity
        self.line(&format!(
            "impl<T: {} + Send + 'static> RpcEntity for {}<T> {{",
            iface_name, service_name
        ));
        self.indent += 1;
        self.line("fn close(&mut self) -> DdsRpcResult<()> { self.inner.close() }");
        self.line("fn is_closed(&self) -> bool { self.inner.is_closed() }");
        self.indent -= 1;
        self.line("}");
    }

    /// (7.11.1.5.4) Generate typed client with per-operation methods.
    /// raises → DdsRpcResult<T, E>, no raises → DdsRpcResult<T>.
    fn emit_client_struct(&mut self, iface_name: &str, qualified_name: &str, ops: &[ResolvedOperation]) {
        let call_type = format!("{}_Call", iface_name);
        let return_type = format!("{}_Return", iface_name);
        let client_name = format!("{}Client", iface_name);

        // Struct definition with default_timeout
        self.line(&format!("pub struct {} {{", client_name));
        self.indent += 1;
        self.line(&format!("client: Client<{}, {}>,", call_type, return_type));
        self.line("default_timeout: Duration,");
        self.indent -= 1;
        self.line("}");
        self.line("");

        // impl block
        self.line(&format!("impl {} {{", client_name));
        self.indent += 1;

        // Constructor (default 10s timeout)
        self.line("pub fn new(params: ClientParams) -> DdsRpcResult<Self> {");
        self.indent += 1;
        self.line(&format!(
            "let client = Client::new(params.interface_name(\"{}\"))?;",
            qualified_name
        ));
        self.line("Ok(Self { client, default_timeout: Duration::from_secs(10) })");
        self.indent -= 1;
        self.line("}");
        self.line("");

        // Constructor with explicit timeout
        self.line("pub fn with_timeout(params: ClientParams, timeout: Duration) -> DdsRpcResult<Self> {");
        self.indent += 1;
        self.line(&format!(
            "let client = Client::new(params.interface_name(\"{}\"))?;",
            qualified_name
        ));
        self.line("Ok(Self { client, default_timeout: timeout })");
        self.indent -= 1;
        self.line("}");

        // Per-operation sync methods
        for op in ops {
            self.line("");
            self.emit_client_method(iface_name, op);
        }

        self.indent -= 1;
        self.line("}");
    }

    fn emit_client_method(&mut self, iface_name: &str, op: &ResolvedOperation) {
        let call_type = format!("{}_Call", iface_name);
        let return_type = format!("{}_Return", iface_name);
        let method_name = to_snake_case(&op.name);
        let variant = naming::to_pascal_case(&op.name);
        let in_type = format!("{}_{}_In", iface_name, op.name);
        let result_type = format!("{}_{}_Result", iface_name, op.name);

        let in_params: Vec<&ResolvedParam> = op.params.iter()
            .filter(|p| matches!(p.direction, ResolvedParamDirection::In | ResolvedParamDirection::Inout))
            .collect();

        let out_params: Vec<&ResolvedParam> = op.params.iter()
            .filter(|p| matches!(p.direction, ResolvedParamDirection::Out | ResolvedParamDirection::Inout))
            .collect();

        // Build method signature params
        let mut sig_params = String::from("&self");
        for p in &in_params {
            let ty = self.type_to_rust(&p.resolved_type);
            sig_params.push_str(&format!(", {}: {}", to_snake_case(&p.name), ty));
        }
        sig_params.push_str(", timeout: Duration");

        // Inner value type (what's extracted from Out struct)
        let inner_ret = Self::client_return_type(op, &out_params, |t| self.type_to_rust(t));

        // Full return type with DdsRpcResult<T> or DdsRpcResult<T, E>
        let ret_sig = if op.raises.is_empty() {
            format!("DdsRpcResult<{}>", inner_ret)
        } else {
            let err_type = Self::error_type_for_op(iface_name, op);
            format!("DdsRpcResult<{}, {}>", inner_ret, err_type)
        };

        self.line(&format!("pub fn {}({}) -> {} {{", method_name, sig_params, ret_sig));
        self.indent += 1;

        // Build In struct
        if in_params.is_empty() {
            self.line(&format!("let call = {}::{}({} {{ dummy: UnusedMember {{}} }});",
                call_type, variant, in_type));
        } else {
            let fields: Vec<String> = in_params.iter()
                .map(|p| to_snake_case(&p.name))
                .collect();
            self.line(&format!("let call = {}::{}({} {{ {} }});",
                call_type, variant, in_type, fields.join(", ")));
        }

        // Send and receive
        self.line("let _id = self.client.send_request(&call).map_err(DdsRpcError::from_untyped)?;");
        self.line("let reply = self.client.receive_reply(timeout).map_err(DdsRpcError::from_untyped)?;");
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
        self.line("match &data.data {");
        self.indent += 1;
        self.line(&format!("{}::{}(result) => match result {{", return_type, variant));
        self.indent += 1;
        self.line(&format!("{}::Result(out) => {{", result_type));
        self.indent += 1;

        // Extract return value
        let has_return = op.return_type.is_some();
        let has_out = !out_params.is_empty();

        if !has_return && !has_out {
            self.line("Ok(())");
        } else {
            let mut fields = Vec::new();
            for p in &out_params {
                fields.push(format!("out.{}.clone()", to_snake_case(&p.name)));
            }
            if has_return {
                let return_name = Self::resolve_return_name(
                    &out_params.iter().map(|p| *p).collect::<Vec<_>>()
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

        // Exception variants → UserException
        if op.raises.is_empty() {
            self.line("_ => Err(DdsRpcError::Remote(RemoteExceptionCode::UnknownException)),");
        } else if op.raises.len() == 1 {
            let exc_name = &op.raises[0];
            let simple = exc_name.rsplit("::").next().unwrap_or(exc_name);
            let member_name = format!("{}_ex", naming::to_pascal_case(simple));
            self.line(&format!(
                "{}::{}(ex) => Err(DdsRpcError::UserException(ex.clone())),",
                result_type, member_name
            ));
            self.line("_ => Err(DdsRpcError::Remote(RemoteExceptionCode::UnknownException)),");
        } else {
            // Multiple exceptions: wrap into error enum
            let error_enum = format!("{}_{}_Error", iface_name, op.name);
            for exc_name in &op.raises {
                let simple = exc_name.rsplit("::").next().unwrap_or(exc_name);
                let pascal = naming::to_pascal_case(simple);
                let member_name = format!("{}_ex", pascal);
                self.line(&format!(
                    "{}::{}(ex) => Err(DdsRpcError::UserException({}::{}(ex.clone()))),",
                    result_type, member_name, error_enum, pascal
                ));
            }
            self.line("_ => Err(DdsRpcError::Remote(RemoteExceptionCode::UnknownException)),");
        }

        self.indent -= 1;
        self.line("}");
        // Wrong operation in return
        self.line("_ => Err(DdsRpcError::Remote(RemoteExceptionCode::UnknownOperation)),");
        self.indent -= 1;
        self.line("}");

        self.indent -= 1;
        self.line("}");
    }

    /// (Step 4) Generate service error enum for operations with 2+ raises.
    fn emit_service_error_enum(&mut self, iface_name: &str, op: &ResolvedOperation) {
        if op.raises.len() < 2 {
            return;
        }

        let enum_name = format!("{}_{}_Error", iface_name, op.name);
        self.line("#[derive(Debug, Clone)]");
        self.line(&format!("pub enum {} {{", enum_name));
        self.indent += 1;
        for exc_name in &op.raises {
            let simple = exc_name.rsplit("::").next().unwrap_or(exc_name);
            let pascal = naming::to_pascal_case(simple);
            self.line(&format!("{}({}),", pascal, pascal));
        }
        self.indent -= 1;
        self.line("}");
        self.line("");
    }

    /// (Step 8) Generate `${Interface}Async` trait (7.11.1.1.2 rule 7).
    fn emit_async_trait(&mut self, iface_name: &str, ops: &[ResolvedOperation]) {
        let trait_name = format!("{}Async", iface_name);
        self.line(&format!("pub trait {} {{", trait_name));
        self.indent += 1;

        for op in ops {
            let method_name = format!("{}_async", to_snake_case(&op.name));
            let future_type = format!("{}_{}_Future", iface_name, op.name);

            // (7.11.1.1.2 rule 7) In/InOut params only, all as immutable references
            let mut sig_params = String::from("&self");
            for p in &op.params {
                match p.direction {
                    ResolvedParamDirection::In | ResolvedParamDirection::Inout => {
                        let ty = self.type_to_rust(&p.resolved_type);
                        if Self::is_copy_type(&p.resolved_type) {
                            sig_params.push_str(&format!(", {}: {}", to_snake_case(&p.name), ty));
                        } else {
                            sig_params.push_str(&format!(", {}: &{}", to_snake_case(&p.name), ty));
                        }
                    }
                    ResolvedParamDirection::Out => {}
                }
            }

            self.line(&format!(
                "fn {}({}) -> DdsRpcResult<{}>;",
                method_name, sig_params, future_type
            ));
        }

        self.indent -= 1;
        self.line("}");
    }

    /// (Step 9) Generate per-operation Future wrapper struct + get/get_timeout.
    fn emit_operation_future(&mut self, iface_name: &str, op: &ResolvedOperation) {
        let return_type = format!("{}_Return", iface_name);
        let result_type = format!("{}_{}_Result", iface_name, op.name);
        let future_name = format!("{}_{}_Future", iface_name, op.name);
        let variant = naming::to_pascal_case(&op.name);

        let out_params: Vec<&ResolvedParam> = op.params.iter()
            .filter(|p| matches!(p.direction, ResolvedParamDirection::Out | ResolvedParamDirection::Inout))
            .collect();

        let inner_ret = Self::client_return_type(op, &out_params, |t| self.type_to_rust(t));

        let ret_sig = if op.raises.is_empty() {
            format!("DdsRpcResult<{}>", inner_ret)
        } else {
            let err_type = Self::error_type_for_op(iface_name, op);
            format!("DdsRpcResult<{}, {}>", inner_ret, err_type)
        };

        // Struct
        self.line(&format!("pub struct {} {{", future_name));
        self.indent += 1;
        self.line(&format!("inner: Future<{}>,", return_type));
        self.indent -= 1;
        self.line("}");
        self.line("");

        // impl with get() and get_timeout()
        self.line(&format!("impl {} {{", future_name));
        self.indent += 1;

        // Helper: emit the reply-unpacking logic (shared by get and get_timeout)
        // We generate two methods with different sample acquisition.
        for (method_sig, get_call) in [
            (format!("pub fn get(self) -> {}", ret_sig), "self.inner.get().map_err(DdsRpcError::from_untyped)?"),
            (format!("pub fn get_timeout(self, timeout: Duration) -> {}", ret_sig), "self.inner.get_timeout(timeout).map_err(DdsRpcError::from_untyped)?"),
        ] {
            self.line(&format!("{} {{", method_sig));
            self.indent += 1;
            self.line(&format!("let sample = {};", get_call));
            self.line("let data = sample.data().map_err(|e| DdsRpcError::Dds(e.into()))?;");
            self.line("if data.header.remote_ex != RemoteExceptionCode::Ok {");
            self.indent += 1;
            self.line("return Err(DdsRpcError::Remote(data.header.remote_ex));");
            self.indent -= 1;
            self.line("}");

            self.line("match &data.data {");
            self.indent += 1;
            self.line(&format!("{}::{}(result) => match result {{", return_type, variant));
            self.indent += 1;
            self.line(&format!("{}::Result(out) => {{", result_type));
            self.indent += 1;

            let has_return = op.return_type.is_some();
            let has_out = !out_params.is_empty();

            if !has_return && !has_out {
                self.line("Ok(())");
            } else {
                let mut fields = Vec::new();
                for p in &out_params {
                    fields.push(format!("out.{}.clone()", to_snake_case(&p.name)));
                }
                if has_return {
                    let return_name = Self::resolve_return_name(
                        &out_params.iter().map(|p| *p).collect::<Vec<_>>()
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
            if op.raises.is_empty() {
                self.line("_ => Err(DdsRpcError::Remote(RemoteExceptionCode::UnknownException)),");
            } else if op.raises.len() == 1 {
                let exc_name = &op.raises[0];
                let simple = exc_name.rsplit("::").next().unwrap_or(exc_name);
                let member_name = format!("{}_ex", naming::to_pascal_case(simple));
                self.line(&format!(
                    "{}::{}(ex) => Err(DdsRpcError::UserException(ex.clone())),",
                    result_type, member_name
                ));
                self.line("_ => Err(DdsRpcError::Remote(RemoteExceptionCode::UnknownException)),");
            } else {
                let error_enum = format!("{}_{}_Error", iface_name, op.name);
                for exc_name in &op.raises {
                    let simple = exc_name.rsplit("::").next().unwrap_or(exc_name);
                    let pascal = naming::to_pascal_case(simple);
                    let member_name = format!("{}_ex", pascal);
                    self.line(&format!(
                        "{}::{}(ex) => Err(DdsRpcError::UserException({}::{}(ex.clone()))),",
                        result_type, member_name, error_enum, pascal
                    ));
                }
                self.line("_ => Err(DdsRpcError::Remote(RemoteExceptionCode::UnknownException)),");
            }

            self.indent -= 1;
            self.line("}");
            self.line("_ => Err(DdsRpcError::Remote(RemoteExceptionCode::UnknownOperation)),");
            self.indent -= 1;
            self.line("}");

            self.indent -= 1;
            self.line("}");
            self.line("");
        }

        self.indent -= 1;
        self.line("}");
    }

    /// (Step 10) Generate `impl ${Interface}Async for ${Interface}Client`.
    fn emit_client_async_impl(&mut self, iface_name: &str, ops: &[ResolvedOperation]) {
        let call_type = format!("{}_Call", iface_name);
        let client_name = format!("{}Client", iface_name);
        let trait_name = format!("{}Async", iface_name);

        self.line(&format!("impl {} for {} {{", trait_name, client_name));
        self.indent += 1;

        for op in ops {
            let method_name = format!("{}_async", to_snake_case(&op.name));
            let future_type = format!("{}_{}_Future", iface_name, op.name);
            let variant = naming::to_pascal_case(&op.name);
            let in_type = format!("{}_{}_In", iface_name, op.name);

            let in_params: Vec<&ResolvedParam> = op.params.iter()
                .filter(|p| matches!(p.direction, ResolvedParamDirection::In | ResolvedParamDirection::Inout))
                .collect();

            // Signature
            let mut sig_params = String::from("&self");
            for p in &in_params {
                let ty = self.type_to_rust(&p.resolved_type);
                if Self::is_copy_type(&p.resolved_type) {
                    sig_params.push_str(&format!(", {}: {}", to_snake_case(&p.name), ty));
                } else {
                    sig_params.push_str(&format!(", {}: &{}", to_snake_case(&p.name), ty));
                }
            }

            self.line(&format!(
                "fn {}({}) -> DdsRpcResult<{}> {{",
                method_name, sig_params, future_type
            ));
            self.indent += 1;

            // Build call value
            if in_params.is_empty() {
                self.line(&format!("let call = {}::{}({} {{ dummy: UnusedMember {{}} }});",
                    call_type, variant, in_type));
            } else {
                let fields: Vec<String> = in_params.iter()
                    .map(|p| {
                        let name = to_snake_case(&p.name);
                        if Self::is_copy_type(&p.resolved_type) {
                            name
                        } else {
                            format!("{}: {}.clone()", name, name)
                        }
                    })
                    .collect();
                self.line(&format!("let call = {}::{}({} {{ {} }});",
                    call_type, variant, in_type, fields.join(", ")));
            }

            self.line("let future = self.client.send_request_async(&call).map_err(DdsRpcError::from_untyped)?;");
            self.line(&format!("Ok({} {{ inner: future }})", future_type));

            self.indent -= 1;
            self.line("}");
        }

        self.indent -= 1;
        self.line("}");
    }

    /// (Step 11) Generate RpcEntity + ServiceProxy + ClientEndpoint impls for the Client struct.
    fn emit_client_endpoint_impl(&mut self, iface_name: &str) {
        let call_type = format!("{}_Call", iface_name);
        let return_type = format!("{}_Return", iface_name);
        let client_name = format!("{}Client", iface_name);

        // RpcEntity
        self.line(&format!("impl RpcEntity for {} {{", client_name));
        self.indent += 1;
        self.line("fn close(&mut self) -> DdsRpcResult<()> { self.client.close() }");
        self.line("fn is_closed(&self) -> bool { self.client.is_closed() }");
        self.indent -= 1;
        self.line("}");
        self.line("");

        // ServiceProxy
        self.line(&format!("impl ServiceProxy for {} {{", client_name));
        self.indent += 1;
        self.line("fn bind_instance(&mut self, name: InstanceName) -> DdsRpcResult<()> { self.client.bind_instance(name) }");
        self.line("fn unbind(&mut self) -> DdsRpcResult<()> { self.client.unbind() }");
        self.line("fn get_bound_instance_name(&self) -> Option<&str> { self.client.get_bound_instance_name() }");
        self.line("fn wait_for_service(&self) -> DdsRpcResult<()> { self.client.wait_for_service() }");
        self.line("fn wait_for_service_timeout(&self, timeout: Duration) -> DdsRpcResult<()> { self.client.wait_for_service_timeout(timeout) }");
        self.indent -= 1;
        self.line("}");
        self.line("");

        // ClientEndpoint
        self.line(&format!("impl ClientEndpoint for {} {{", client_name));
        self.indent += 1;
        self.line(&format!("type TReq = {};", call_type));
        self.line(&format!("type TRep = {};", return_type));
        self.line(&format!(
            "fn get_request_datawriter(&self) -> DdsRpcResult<&{}::dcps::publication::data_writer::DataWriter<{}_rpc::types::Request<Self::TReq>>> {{",
            self.opts.crate_path, self.opts.crate_path
        ));
        self.indent += 1;
        self.line("self.client.get_request_datawriter()");
        self.indent -= 1;
        self.line("}");
        self.line(&format!(
            "fn get_reply_datareader(&self) -> DdsRpcResult<&{}::dcps::subscription::data_reader::DataReader<{}_rpc::types::Reply<Self::TRep>>> {{",
            self.opts.crate_path, self.opts.crate_path
        ));
        self.indent += 1;
        self.line("self.client.get_reply_datareader()");
        self.indent -= 1;
        self.line("}");
        self.indent -= 1;
        self.line("}");
    }

    /// Build out_fields vec for dispatcher use.
    fn build_out_fields(op: &ResolvedOperation, out_params: &[&ResolvedParam]) -> Vec<String> {
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
            let return_name = Self::resolve_return_name(
                &out_params.iter().map(|p| *p).collect::<Vec<_>>()
            );
            out_fields.push(format!("{}: return_", return_name));
        }
        out_fields
    }

    /// Get the error type name for an operation with raises.
    /// 1 exception → exception type directly; 2+ → generated error enum.
    fn error_type_for_op(iface_name: &str, op: &ResolvedOperation) -> String {
        if op.raises.len() == 1 {
            let exc_name = &op.raises[0];
            let simple = exc_name.rsplit("::").next().unwrap_or(exc_name);
            naming::to_pascal_case(simple)
        } else {
            format!("{}_{}_Error", iface_name, op.name)
        }
    }

    /// Compute the inner return type for client/future methods.
    fn client_return_type<F: Fn(&ResolvedType) -> String>(
        op: &ResolvedOperation,
        out_params: &[&ResolvedParam],
        type_to_rust: F,
    ) -> String {
        let has_return = op.return_type.is_some();
        let has_out = !out_params.is_empty();

        if !has_return && !has_out {
            "()".to_string()
        } else {
            let mut parts = Vec::new();
            for p in out_params {
                parts.push(type_to_rust(&p.resolved_type));
            }
            if let Some(ret) = &op.return_type {
                parts.push(type_to_rust(ret));
            }
            if parts.len() == 1 {
                parts[0].clone()
            } else {
                format!("({})", parts.join(", "))
            }
        }
    }

    fn is_copy_type(ty: &ResolvedType) -> bool {
        matches!(ty,
            ResolvedType::Bool | ResolvedType::U8 | ResolvedType::I8 |
            ResolvedType::I16 | ResolvedType::U16 | ResolvedType::I32 |
            ResolvedType::U32 | ResolvedType::I64 | ResolvedType::U64 |
            ResolvedType::F32 | ResolvedType::F64 | ResolvedType::Char |
            ResolvedType::WChar
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
        let start = code.find(header).unwrap_or_else(|| panic!("'{}' not found in:\n{}", header, code));
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
        assert!(code.contains(&format!("pub const ROBOT_CONTROL_SET_SPEED_HASH: i32 = {};", set_hash)));
        assert!(code.contains(&format!("pub const ROBOT_CONTROL_GET_SPEED_HASH: i32 = {};", get_hash)));

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
        assert!(body.contains("SetSpeed(RobotControl_setSpeed_Result) = ROBOT_CONTROL_SET_SPEED_HASH,"));
        assert!(body.contains("GetSpeed(RobotControl_getSpeed_Result) = ROBOT_CONTROL_GET_SPEED_HASH,"));
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
        assert!(call.contains("GetStatus(RobotControl_getStatus_In) = ROBOT_CONTROL_GET_STATUS_HASH,"));

        // Return union (7.5.1.1.7)
        let ret = extract_body(&code, "pub enum RobotControl_Return {");
        assert!(ret.contains("UnknownOp(UnknownOperation) = -1,"));
        assert!(ret.contains("Command(RobotControl_command_Result) = ROBOT_CONTROL_COMMAND_HASH,"));
        assert!(ret.contains("SetSpeed(RobotControl_setSpeed_Result) = ROBOT_CONTROL_SET_SPEED_HASH,"));
        assert!(ret.contains("GetSpeed(RobotControl_getSpeed_Result) = ROBOT_CONTROL_GET_SPEED_HASH,"));
        assert!(ret.contains("GetStatus(RobotControl_getStatus_Result) = ROBOT_CONTROL_GET_STATUS_HASH,"));

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
        assert!(code.contains(&format!(
            "pub const CALCULATOR_ON_HASH: i32 = {};",
            rpc_hash("on")
        )));
        assert!(code.contains(&format!(
            "pub const CALCULATOR_OFF_HASH: i32 = {};",
            rpc_hash("off")
        )));

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
        assert!(code.contains("fn handle_request(&self, request: &RobotControl_Call) -> (RobotControl_Return, RemoteExceptionCode)"));
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
        assert!(code.contains("pub fn set_speed(&self, speed: f32, timeout: Duration) -> DdsRpcResult<()>"));
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
        assert!(code.contains("pub fn compute(&self, x: i32, timeout: Duration) -> DdsRpcResult<(f64, i32)>"));
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
        assert!(code.contains("fn set_speed(&self, speed: f32) -> Result<(), TooFast>;"));
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
        assert!(code.contains("pub fn command(&self, com: Command, timeout: Duration) -> DdsRpcResult<()>"));
        assert!(code.contains("pub fn set_speed(&self, speed: f32, timeout: Duration) -> DdsRpcResult<(), TooFast>"));
        assert!(code.contains("pub fn get_speed(&self, timeout: Duration) -> DdsRpcResult<f32>"));
        assert!(code.contains("pub fn get_status(&self, timeout: Duration) -> DdsRpcResult<i32>"));
    }

    #[test]
    fn test_service_error_enum() {
        let defs = parse_idl(
            r#"
            exception TooFast { float speed; };
            exception InvalidInput { string reason; };
            interface Robot {
                void navigate(in float x) raises (TooFast, InvalidInput);
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        // 2+ raises → error enum generated
        assert!(code.contains("pub enum Robot_navigate_Error {"));
        assert!(code.contains("TooFast(TooFast),"));
        assert!(code.contains("InvalidInput(InvalidInput),"));
    }

    #[test]
    fn test_no_error_enum_single_raises() {
        let defs = parse_idl(
            r#"
            exception TooFast { float speed; };
            interface Robot {
                void setSpeed(in float speed) raises (TooFast);
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        // 1 raises → no error enum
        assert!(!code.contains("Robot_setSpeed_Error"));
    }

    #[test]
    fn test_async_trait() {
        let defs = parse_idl(
            r#"
            exception TooFast { float speed; };
            interface RobotControl {
                void setSpeed(in float speed) raises (TooFast);
                float getSpeed();
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        assert!(code.contains("pub trait RobotControlAsync {"));
        assert!(code.contains("fn set_speed_async(&self, speed: f32) -> DdsRpcResult<RobotControl_setSpeed_Future>;"));
        assert!(code.contains("fn get_speed_async(&self) -> DdsRpcResult<RobotControl_getSpeed_Future>;"));
    }

    #[test]
    fn test_async_trait_ref_params() {
        let defs = parse_idl(
            r#"
            interface Foo {
                void bar(in string name);
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        // Non-copy type → immutable reference in async trait
        assert!(code.contains("fn bar_async(&self, name: &String) -> DdsRpcResult<Foo_bar_Future>;"));
    }

    #[test]
    fn test_operation_future() {
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

        assert!(code.contains("pub struct Foo_getSpeed_Future {"));
        assert!(code.contains("inner: Future<Foo_Return>,"));
        assert!(code.contains("pub fn get(self) -> DdsRpcResult<f32>"));
        assert!(code.contains("pub fn get_timeout(self, timeout: Duration) -> DdsRpcResult<f32>"));
    }

    #[test]
    fn test_operation_future_with_raises() {
        let defs = parse_idl(
            r#"
            exception TooFast { float speed; };
            interface Foo {
                void setSpeed(in float speed) raises (TooFast);
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        assert!(code.contains("pub struct Foo_setSpeed_Future {"));
        assert!(code.contains("pub fn get(self) -> DdsRpcResult<(), TooFast>"));

        // Body: exception variant → UserException
        assert!(code.contains("Foo_setSpeed_Result::TooFast_ex(ex) => Err(DdsRpcError::UserException(ex.clone())),"));
        // Body: remote exception check
        assert!(code.contains("if data.header.remote_ex != RemoteExceptionCode::Ok {"));
    }

    #[test]
    fn test_client_async_impl() {
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

        assert!(code.contains("impl FooAsync for FooClient {"));
        assert!(code.contains("fn get_speed_async(&self) -> DdsRpcResult<Foo_getSpeed_Future>"));
        assert!(code.contains("Ok(Foo_getSpeed_Future { inner: future })"));
    }

    #[test]
    fn test_client_endpoint_impl() {
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

        assert!(code.contains("impl RpcEntity for FooClient {"));
        assert!(code.contains("impl ServiceProxy for FooClient {"));
        assert!(code.contains("impl ClientEndpoint for FooClient {"));
        assert!(code.contains("type TReq = Foo_Call;"));
        assert!(code.contains("type TRep = Foo_Return;"));
    }

    #[test]
    fn test_unknown_op_handling() {
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

        // (7.7.1) UnknownOp → Unsupported
        assert!(code.contains(
            "Foo_Call::UnknownOp(_) => (Foo_Return::UnknownOp(UnknownOperation {}), RemoteExceptionCode::Unsupported),"
        ));
    }

    #[test]
    fn test_exception_dispatch_single() {
        let defs = parse_idl(
            r#"
            exception TooFast { float speed; };
            interface Robot {
                void setSpeed(in float speed) raises (TooFast);
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        // Dispatcher should match on Result and map Err to exception variant
        assert!(code.contains("match self.inner.set_speed("));
        assert!(code.contains("Err(ex) => (Robot_Return::SetSpeed(Robot_setSpeed_Result::TooFast_ex(ex)), RemoteExceptionCode::Ok),"));
    }

    #[test]
    fn test_exception_dispatch_multiple() {
        let defs = parse_idl(
            r#"
            exception TooFast { float speed; };
            exception InvalidInput { string reason; };
            interface Robot {
                void navigate(in float x) raises (TooFast, InvalidInput);
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        // Dispatcher should match on error enum variants
        assert!(code.contains("Err(err) => match err {"));
        assert!(code.contains("Robot_navigate_Error::TooFast(ex)"));
        assert!(code.contains("Robot_navigate_Error::InvalidInput(ex)"));
    }


    #[test]
    fn test_default_timeout_field() {
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

        assert!(code.contains("default_timeout: Duration,"));
        assert!(code.contains("default_timeout: Duration::from_secs(10)"));
        assert!(code.contains("pub fn with_timeout(params: ClientParams, timeout: Duration) -> DdsRpcResult<Self>"));
    }

    #[test]
    fn test_service_trait_raises_result() {
        let defs = parse_idl(
            r#"
            exception TooFast { float speed; };
            exception InvalidInput { string reason; };
            interface Robot {
                void command();
                void setSpeed(in float speed) raises (TooFast);
                void navigate(in float x) raises (TooFast, InvalidInput);
                float getSpeed();
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        // No raises → no Result wrapper
        assert!(code.contains("fn command(&self);"));
        assert!(code.contains("fn get_speed(&self) -> f32;"));
        // 1 raises → Result<T, ExType>
        assert!(code.contains("fn set_speed(&self, speed: f32) -> Result<(), TooFast>;"));
        // 2+ raises → Result<T, ErrorEnum>
        assert!(code.contains("fn navigate(&self, x: f32) -> Result<(), Robot_navigate_Error>;"));
    }

    #[test]
    fn test_type_to_rust_collections() {
        let defs = parse_idl(
            r#"
            interface Foo {
                void bar(in sequence<long> ids, in sequence<double, 10> bounded);
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        let body = extract_body(&code, "pub struct Foo_bar_In {");
        assert!(body.contains("pub ids: Vec<i32>,"));
        assert!(body.contains("pub bounded: Vec<f64>,"));
    }

    #[test]
    fn test_dispatcher_body_out_and_inout_params() {
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

        // out param → Default::default() initialization
        assert!(code.contains("let mut y = Default::default();"));
        // inout param → clone + _mut
        assert!(code.contains("let mut z_mut = z.clone();"));
        // call with correct references
        assert!(code.contains("self.inner.compute(*x, &mut y, &mut z_mut)"));
        // Out struct field mapping
        assert!(code.contains("y: y"));
        assert!(code.contains("z: z_mut"));
    }

    #[test]
    fn test_client_body_multi_exception_mapping() {
        let defs = parse_idl(
            r#"
            exception TooFast { float speed; };
            exception InvalidInput { string reason; };
            interface Robot {
                void navigate(in float x) raises (TooFast, InvalidInput);
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        // Client method body: each exception → UserException(ErrorEnum::Variant)
        assert!(code.contains(
            "Robot_navigate_Result::TooFast_ex(ex) => Err(DdsRpcError::UserException(Robot_navigate_Error::TooFast(ex.clone()))),"
        ));
        assert!(code.contains(
            "Robot_navigate_Result::InvalidInput_ex(ex) => Err(DdsRpcError::UserException(Robot_navigate_Error::InvalidInput(ex.clone()))),"
        ));
        // Remote exception code check
        assert!(code.contains("if data.header.remote_ex != RemoteExceptionCode::Ok {"));
        assert!(code.contains("return Err(DdsRpcError::Remote(data.header.remote_ex));"));
    }

    #[test]
    fn test_client_async_impl_non_copy_clone() {
        let defs = parse_idl(
            r#"
            interface Foo {
                void bar(in string name, in long id);
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let code = generate(&model, &RpcOptions::default());

        // Async impl: non-copy param (String) → .clone(), copy param (i32) → as-is
        assert!(code.contains("impl FooAsync for FooClient {"));
        assert!(code.contains("fn bar_async(&self, name: &String, id: i32) -> DdsRpcResult<Foo_bar_Future>"));
        assert!(code.contains("name: name.clone()"));
    }
}
