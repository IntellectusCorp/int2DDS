//! Function-call style Server — container for Services (7.11.1.5.3)

use std::time::{Duration, Instant};

use crate::entity::RpcEntity;
use crate::error::DdsRpcResult;
use crate::service::{ServiceParams, ServiceStatus};

/// Type-erased dispatch trait for heterogeneous Service storage.
/// Service<TReq, TRep, H> implements this trait, allowing Server
/// to hold services with different type parameters in a single Vec.
pub trait Dispatchable: RpcEntity + Send {
    fn try_dispatch_one(&self) -> DdsRpcResult<bool>;
    fn status(&self) -> ServiceStatus;
}

pub struct ServerParams {
    pub(crate) default_service_params: Option<ServiceParams>,
}

impl ServerParams {
    pub fn new() -> Self {
        Self { default_service_params: None }
    }

    pub fn default_service_params(mut self, params: ServiceParams) -> Self {
        self.default_service_params = Some(params);
        self
    }
}

impl Default for ServerParams {
    fn default() -> Self {
        Self::new()
    }
}

/// Container of one or more Services (7.11.1.5.3).
/// Provides blocking and time-limited dispatch loops that poll
/// all registered services for incoming requests.
pub struct Server {
    services: Vec<Box<dyn Dispatchable>>,
    closed: bool,
}

impl Server {
    pub fn new(_params: ServerParams) -> Self {
        Self { services: Vec::new(), closed: false }
    }

    /// Register a Service with this Server.
    pub fn add_service<S: Dispatchable + 'static>(&mut self, service: S) {
        self.services.push(Box::new(service));
    }

    /// Blocking dispatch loop. Polls all services for requests
    /// until `close()` is called.
    pub fn run(&self) -> DdsRpcResult<()> {
        loop {
            if self.closed {
                return Ok(());
            }
            let mut any_processed = false;
            for service in &self.services {
                if service.is_closed() {
                    continue;
                }
                if service.try_dispatch_one()? {
                    any_processed = true;
                }
            }
            if !any_processed {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    /// Time-limited dispatch loop. Polls all services for requests
    /// until `max_wait` elapses.
    pub fn run_for(&self, max_wait: Duration) -> DdsRpcResult<()> {
        let start = Instant::now();
        loop {
            if self.closed || start.elapsed() >= max_wait {
                return Ok(());
            }
            let mut any_processed = false;
            for service in &self.services {
                if service.is_closed() {
                    continue;
                }
                if service.try_dispatch_one()? {
                    any_processed = true;
                }
            }
            if !any_processed {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

impl RpcEntity for Server {
    fn close(&mut self) -> DdsRpcResult<()> {
        for service in &mut self.services {
            if !service.is_closed() {
                service.close()?;
            }
        }
        self.closed = true;
        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.closed
    }
}
