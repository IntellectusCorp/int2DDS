//! Entity - Base trait for all DDS entities.
//!
//! This module defines the `Entity` trait hierarchy that forms the foundation of all
//! DDS objects. All DDS entities (DomainParticipant, Publisher, Subscriber, DataWriter,
//! DataReader, Topic) implement these traits, providing common functionality for:
//!
//! - Instance handles for unique identification
//! - Enable/disable lifecycle management
//! - QoS policy management
//! - Status condition access
//! - Status change monitoring
//!
//! The trait hierarchy includes `BaseEntity` for core functionality and `Entity` for
//! QoS-aware entities.

use std::{
    fmt::Debug,
    marker::PhantomData,
    sync::{Arc, Mutex, MutexGuard},
    thread::ThreadId,
};

use crate::{
    common::instance_handle::InstanceHandle,
    core::error::{DdsError, DdsResult},
    infrastructure::status::{StatusInfo, StatusMask},
};

use super::{status::StatusKind, status_condition::StatusCondition};

#[derive(Debug, Default)]
struct LifecycleState {
    is_deleted: bool,
    // The thread of every admitted operation, one entry per nesting level.
    holder_threads: Vec<ThreadId>,
}

// Serializes an entity's teardown against its in-flight public operations. Delete marks the
// entity deleted, then drains the holders before teardown proceeds.
#[derive(Debug, Default)]
pub(crate) struct EntityLifecycle {
    state: Mutex<LifecycleState>,
}

impl EntityLifecycle {
    pub(crate) fn is_deleted(&self) -> bool {
        self.lock_state().is_deleted
    }

    // Admits one public operation and records its thread until the guard drops. Once the entity
    // is deleted only a thread already holding an operation is admitted, so the holders drain.
    pub(crate) fn begin_operation(&self) -> DdsResult<OperationGuard<'_>> {
        let mut state = self.lock_state();
        let current_thread = std::thread::current().id();

        if state.is_deleted && !state.holder_threads.contains(&current_thread) {
            debug!(
                "EntityLifecycle::begin_operation() - operation refused, entity already deleted"
            );
            return Err(DdsError::AlreadyDeleted);
        }

        state.holder_threads.push(current_thread);
        // debug!(
        //     "EntityLifecycle::begin_operation() - active_operation_count = {}",
        //     state.holder_threads.len()
        // );

        Ok(OperationGuard { lifecycle: self, thread: current_thread, _not_send: PhantomData })
    }

    // Marks the entity deleted, then blocks until admitted operations finish. A listener
    // callback caller skips the wait, since it holds a lease only it could release.
    pub(crate) fn mark_deleted_and_await_operation_completion(&self) {
        self.lock_state().is_deleted = true;
        debug!("Marked entity as deleted, waiting for operations to complete");

        if crate::utils::notify::in_listener_callback() {
            debug!("Cannot delete entity while in listener callback");
            return;
        }

        // Poll cadence matches the rtps callback drain.
        while self.get_active_operation_count() > 0 {
            std::thread::sleep(std::time::Duration::from_micros(50));
        }

        debug!("All operations completed, entity can be safely deleted");
    }

    fn get_active_operation_count(&self) -> usize {
        self.lock_state().holder_threads.len()
    }

    fn lock_state(&self) -> MutexGuard<'_, LifecycleState> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

// Holds this thread's entry for one public operation. Drop removes it, so every early return
// (including `?`) releases the entity to a waiting delete.
pub(crate) struct OperationGuard<'a> {
    lifecycle: &'a EntityLifecycle,
    thread: ThreadId,
    // Keeps the guard on the thread that took it, so Drop removes that thread's entry.
    _not_send: PhantomData<*const ()>,
}

impl Drop for OperationGuard<'_> {
    fn drop(&mut self) {
        let mut state = self.lifecycle.lock_state();

        if let Some(position) = state.holder_threads.iter().position(|id| *id == self.thread) {
            state.holder_threads.swap_remove(position);
        }
    }
}

pub trait BaseEntity: Send + Sync + Debug {
    fn get_status_changes(&self) -> DdsResult<StatusMask>;
    fn enable(&self) -> DdsResult<()>;
    fn get_instance_handle(&self) -> DdsResult<InstanceHandle>;
}

pub trait Entity: BaseEntity {
    type Qos: Debug;
    fn get_statuscondition(&self) -> DdsResult<StatusCondition<Self::Qos>>;
    fn set_qos(&self, qos: Self::Qos) -> DdsResult<()>;
    fn get_qos(&self) -> DdsResult<Self::Qos>;
    fn get_qos_arc(&self) -> DdsResult<Arc<Self::Qos>>;
}

pub(crate) trait EnableChild: Entity {
    fn enable_child_entities(&self) -> DdsResult<()> {
        Ok(())
    }
    fn enable_rtps_entities(&self) -> DdsResult<()> {
        Ok(())
    }
    fn update_rtps_entity(&self, _qos: &Self::Qos) -> DdsResult<()> {
        Ok(())
    }
    fn check_parent_enabled(&self) -> DdsResult<()> {
        Ok(())
    }
}

pub(crate) trait UpdateStatus: Entity {
    fn update_status(
        &self,
        status: StatusKind,
        _info: Option<Arc<dyn StatusInfo>>,
    ) -> DdsResult<()> {
        log::debug!(
            "Entity [{:?}] - Status {:?} handled by child entity, no action required at this level",
            self.get_instance_handle()?.to_guid(),
            status
        );
        Ok(())
    }
    fn set_communication_status(
        &self,
        status_kind: &StatusKind,
        trigger_value: bool,
    ) -> DdsResult<()> {
        self.get_statuscondition()?.set_communication_status(status_kind, trigger_value)
    }
}

pub(crate) trait EntityInternal: UpdateStatus + EnableChild {
    fn clone_box(&self) -> Box<dyn Entity<Qos = Self::Qos> + Send + Sync>;
    fn get_listener_mask(&self) -> DdsResult<StatusMask>;
}

macro_rules! impl_dds_entity_impl {
    ($type:ty, $qos_type:ty, {$($impl_generics:tt)*}, {$($where_clause:tt)*}) => {
        impl<$($impl_generics)*> BaseEntity for $type
        where
            $($where_clause)*
        {
            fn get_status_changes(&self) -> DdsResult<StatusMask> {
                let _operation = self.lifecycle.begin_operation()?;

                // Read the changed-status mask under the lock without materializing a clone.
                let status_condition = self
                    .status_condition
                    .lock()
                    .map_err(|e| DdsError::Error(e.to_string()))?;
                status_condition.get_status_changes()
            }

            fn enable(&self) -> DdsResult<()> {
                if self.is_enabled().is_ok() {
                    return Ok(());
                }

                self.check_parent_enabled()?;

                let qos = self.get_qos_arc()?;
                qos.check_unsupported_policies()?;
                qos.is_consistent()?;

                self.enable_rtps_entities()?;
                self.enabled.store(true, Ordering::SeqCst);

                if qos.autoenable_created_entities() {
                    self.enable_child_entities()?;
                }
                Ok(())
            }

            fn get_instance_handle(&self) -> DdsResult<InstanceHandle> {
                let _operation = self.lifecycle.begin_operation()?;
                Ok(InstanceHandle::from_guid(&self.guid))
            }
        }

        impl<$($impl_generics)*> Entity for $type
        where
            $($where_clause)*
        {
            type Qos = $qos_type;

            fn get_statuscondition(&self) -> DdsResult<StatusCondition<Self::Qos>> {
                let _operation = self.lifecycle.begin_operation()?;
                let status_condition =
                    self.status_condition.lock().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(status_condition.clone())
            }

            fn set_qos(&self, qos: Self::Qos) -> DdsResult<()> {
                if self.is_builtin {
                    return Err(DdsError::PreconditionNotMet);
                }
                let _operation = self.lifecycle.begin_operation()?;
                qos.check_unsupported_policies()?;
                qos.is_consistent()?;

                if self.is_enabled().is_ok() {
                    self.get_qos_arc()?.check_immutable_change(&qos)?;
                }

                // Allow only one set_qos to run at a time so that the cache store
                // and the following update_rtps_entity stay paired. Reads via
                // get_qos() go through ArcSwap and never block on this lock.
                let _update_guard = self.update_lock.lock()
                    .map_err(|e| DdsError::Error(e.to_string()))?;

                self.qos.store(std::sync::Arc::new(qos.clone()));
                self.update_rtps_entity(&qos)?;
                Ok(())
            }

            fn get_qos(&self) -> DdsResult<Self::Qos> {
                let _operation = self.lifecycle.begin_operation()?;
                Ok((**self.qos.load()).clone())
            }

            fn get_qos_arc(&self) -> DdsResult<std::sync::Arc<Self::Qos>> {
                Ok(self.qos.load_full())
            }
        }

        impl<$($impl_generics)*> EntityInternal for $type
        where
            $($where_clause)*
        {
            fn clone_box(&self) -> Box<dyn Entity<Qos = Self::Qos> + Send + Sync> {
                Box::new(self.clone())
            }

            fn get_listener_mask(&self) -> DdsResult<StatusMask> {
                Ok(self.mask.read().map_err(|e| DdsError::Error(e.to_string()))?.clone())
            }
        }

        // Public wrapper methods for generic types - can be called directly without importing trait
        impl<$($impl_generics)*> $type
        where
            $($where_clause)*
        {
            /// Wrapper for BaseEntity::get_status_changes()
            #[inline]
            pub fn get_status_changes(&self) -> DdsResult<StatusMask> {
                <Self as BaseEntity>::get_status_changes(self)
            }

            /// Wrapper for BaseEntity::enable()
            #[inline]
            pub fn enable(&self) -> DdsResult<()> {
                <Self as BaseEntity>::enable(self)
            }

            /// Wrapper for BaseEntity::get_instance_handle()
            #[inline]
            pub fn get_instance_handle(&self) -> DdsResult<InstanceHandle> {
                <Self as BaseEntity>::get_instance_handle(self)
            }

            /// Wrapper for Entity::get_statuscondition()
            #[inline]
            pub fn get_statuscondition(&self) -> DdsResult<StatusCondition<$qos_type>> {
                <Self as Entity>::get_statuscondition(self)
            }

            /// Wrapper for Entity::set_qos()
            #[inline]
            pub fn set_qos(&self, qos: $qos_type) -> DdsResult<()> {
                <Self as Entity>::set_qos(self, qos)
            }

            /// Wrapper for Entity::get_qos()
            #[inline]
            pub fn get_qos(&self) -> DdsResult<$qos_type> {
                <Self as Entity>::get_qos(self)
            }

            // Shared read-only handle to the current QoS. Avoids a deep clone
            // on hot paths that only read a field.
            #[inline]
            pub(crate) fn get_qos_arc(&self) -> DdsResult<std::sync::Arc<$qos_type>> {
                <Self as Entity>::get_qos_arc(self)
            }
        }
    };
}

// Extend macro to support generic types
macro_rules! impl_dds_entity {
    // Existing version without generics
    ($type:ty, $qos_type:ty) => {
        impl_dds_entity_impl!($type, $qos_type, {}, {});
    };

    // Version with generics
    ($type:ty, $qos_type:ty, $($generic:tt)*) => {
        impl_dds_entity_impl!($type, $qos_type, {$($generic)*}, {$($generic)*});
    };
}

macro_rules! impl_check_parent_enabled {
    ($parent_getter:ident) => {
        fn check_parent_enabled(&self) -> DdsResult<()> {
            match self.$parent_getter()?.is_enabled() {
                Ok(()) => Ok(()),
                Err(DdsError::NotEnabled) => Err(DdsError::PreconditionNotMet),
                Err(e) => Err(e),
            }
        }
    };
}

pub(crate) use impl_check_parent_enabled;
pub(crate) use impl_dds_entity;
pub(crate) use impl_dds_entity_impl;
use log::debug;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn begin_operation_raises_count_and_guard_drop_lowers_it() {
        let lifecycle = EntityLifecycle::default();

        {
            let _first = lifecycle.begin_operation().expect("admitted while alive");
            assert_eq!(lifecycle.get_active_operation_count(), 1);

            let _second = lifecycle.begin_operation().expect("admitted while alive");
            assert_eq!(lifecycle.get_active_operation_count(), 2);
        }

        assert_eq!(lifecycle.get_active_operation_count(), 0);
    }

    #[test]
    fn begin_operation_is_refused_after_deletion_and_leaves_count_at_zero() {
        let lifecycle = EntityLifecycle::default();
        lifecycle.mark_deleted_and_await_operation_completion();

        assert!(matches!(lifecycle.begin_operation(), Err(DdsError::AlreadyDeleted)));
        assert_eq!(lifecycle.get_active_operation_count(), 0);
    }

    #[test]
    fn an_operation_reached_from_inside_another_is_admitted_after_deletion_begins() {
        let lifecycle = Arc::new(EntityLifecycle::default());
        let outer = lifecycle.begin_operation().expect("admitted while alive");

        let deleter_lifecycle = lifecycle.clone();
        let deleter = thread::spawn(move || {
            deleter_lifecycle.mark_deleted_and_await_operation_completion();
        });

        // Let the deleter mark deleted and settle into its drain before the nested call.
        thread::sleep(Duration::from_millis(20));

        let nested = lifecycle
            .begin_operation()
            .expect("a call reached from inside an admitted operation must not be refused");
        assert_eq!(lifecycle.get_active_operation_count(), 2);

        drop(nested);
        drop(outer);
        deleter.join().expect("deleter thread panicked");

        assert_eq!(lifecycle.get_active_operation_count(), 0);
        assert!(matches!(lifecycle.begin_operation(), Err(DdsError::AlreadyDeleted)));
    }

    #[test]
    fn an_operation_from_another_thread_is_refused_after_deletion_begins() {
        let lifecycle = Arc::new(EntityLifecycle::default());
        let held = lifecycle.begin_operation().expect("admitted while alive");

        let deleter_lifecycle = lifecycle.clone();
        let deleter = thread::spawn(move || {
            deleter_lifecycle.mark_deleted_and_await_operation_completion();
        });

        // Our guard keeps the deleter in its drain while the outsider tries to enter.
        thread::sleep(Duration::from_millis(20));

        let outsider_lifecycle = lifecycle.clone();
        let outsider = thread::spawn(move || outsider_lifecycle.begin_operation().map(|_| ()));
        let outcome = outsider.join().expect("outsider thread panicked");

        assert!(
            matches!(outcome, Err(DdsError::AlreadyDeleted)),
            "a call from another thread was admitted after deletion began, so the drain has no guaranteed end"
        );

        drop(held);
        deleter.join().expect("deleter thread panicked");
    }

    #[test]
    fn deletion_blocks_until_an_in_flight_operation_finishes() {
        let lifecycle = Arc::new(EntityLifecycle::default());

        let worker_lifecycle = lifecycle.clone();
        let operation_finished = Arc::new(AtomicBool::new(false));
        let worker_finished = operation_finished.clone();

        let worker = thread::spawn(move || {
            let _operation = worker_lifecycle.begin_operation().expect("admitted while alive");
            thread::sleep(Duration::from_millis(50));
            worker_finished.store(true, Ordering::SeqCst);
        });

        // Let the worker acquire its guard before deletion starts draining.
        thread::sleep(Duration::from_millis(10));
        lifecycle.mark_deleted_and_await_operation_completion();

        assert!(
            operation_finished.load(Ordering::SeqCst),
            "deletion returned before the in-flight operation finished"
        );

        worker.join().expect("worker thread panicked");
    }
}
