//! Common traits shared by all RPC entities (Requester, Replier, Client, Service).
//!
//! Defines lifecycle operations (close, finalize) and service-discovery
//! helpers (wait_for_service, wait_for_requests).

use crate::error::DdsRpcResult;
use crate::types::InstanceName;
use std::time::Duration;

/// Base trait for all active RPC entities
pub trait RpcEntity {
    fn close(&mut self) -> DdsRpcResult<()>;
    fn is_closed(&self) -> bool;
}

/// Provides service instance binding and discovery waiting.
pub trait ServiceProxy: RpcEntity {
    fn bind_instance(&mut self, instance_name: InstanceName) -> DdsRpcResult<()>;
    fn unbind(&mut self) -> DdsRpcResult<()>;
    fn get_bound_instance_name(&self) -> Option<&str>;
    fn wait_for_service(&self) -> DdsRpcResult<()>;
    fn wait_for_service_timeout(&self, timeout: Duration) -> DdsRpcResult<()>;
}
