//! RPC entity traits (Spec 7.11.1.4.2, 7.11.1.4.4)

use crate::error::DdsRpcResult;
use crate::types::InstanceName;
use std::time::Duration;

/// Base trait for all active RPC entities (Spec 7.11.1.4.2)
pub trait RpcEntity {
    fn close(&mut self) -> DdsRpcResult<()>;
    fn is_closed(&self) -> bool;
}

/// Type-independent operations for Requester and Client (Spec 7.11.1.4.4)
///
/// Provides service instance binding and discovery waiting.
/// Not instantiated directly — implemented by Requester and Client.
pub trait ServiceProxy: RpcEntity {
    fn bind_instance(&mut self, instance_name: InstanceName) -> DdsRpcResult<()>;
    fn unbind(&mut self) -> DdsRpcResult<()>;
    fn get_bound_instance_name(&self) -> Option<&str>;
    fn wait_for_service(&self) -> DdsRpcResult<()>;
    fn wait_for_service_timeout(&self, timeout: Duration) -> DdsRpcResult<()>;
}
