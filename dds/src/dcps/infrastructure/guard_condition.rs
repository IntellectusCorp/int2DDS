//! GuardCondition - Application-controlled condition for WaitSet synchronization.
//!
//! A `GuardCondition` is a manually-triggered condition that allows applications to
//! wake up threads waiting on a `WaitSet`. Unlike other conditions that are triggered
//! by DDS events, a GuardCondition's trigger value is explicitly controlled by the
//! application through `set_trigger_value()`.
//!
//! This is useful for integrating application-specific events with DDS event handling,
//! allowing a unified wait mechanism for both DDS and non-DDS events.

use std::{
    any::Any,
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use crate::{
    core::error::DdsResult,
    infrastructure::condition::{impl_dds_condition, impl_dds_condition_impl, ConditionInternal},
};

use super::condition::Condition;

pub struct GuardCondition {
    trigger_value: Arc<AtomicBool>,
    waitset_callback: Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>,
}

impl From<GuardCondition> for Arc<dyn Condition + Send + Sync>
where
    GuardCondition: Condition + Send + Sync + 'static,
{
    fn from(condition: GuardCondition) -> Self {
        Arc::new(condition) as Arc<dyn Condition + Send + Sync>
    }
}

impl Debug for GuardCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GuardCondition {{ trigger_value: {:?}}}", self.trigger_value)
    }
}

impl_dds_condition!(GuardCondition);
impl Condition for GuardCondition {
    fn get_trigger_value(&self) -> DdsResult<bool> {
        Ok(self.trigger_value.load(Ordering::SeqCst))
    }
}

impl Default for GuardCondition {
    fn default() -> Self {
        Self::new()
    }
}

impl GuardCondition {
    pub fn new() -> Self {
        Self {
            trigger_value: Arc::new(AtomicBool::new(false)),
            waitset_callback: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_trigger_value(&self, value: bool) -> DdsResult<()> {
        self.trigger_value.store(value, Ordering::Relaxed);
        if value {
            if let Ok(callback) = self.waitset_callback.lock() {
                if let Some(callback) = callback.as_ref() {
                    callback();
                }
            }
        }
        Ok(())
    }
}
