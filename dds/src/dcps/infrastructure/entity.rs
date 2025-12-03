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

use std::{fmt::Debug, sync::Arc};

use crate::{
    common::instance_handle::InstanceHandle,
    core::error::DdsResult,
    infrastructure::status::{StatusInfo, StatusMask},
};

use super::{status::StatusKind, status_condition::StatusCondition};

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
                self.is_deleted()?;
                self.get_statuscondition()?.get_status_changes()
            }

            fn enable(&self) -> DdsResult<()> {
                if self.is_enabled().is_ok() {
                    return Ok(());
                }

                self.check_parent_enabled()?;

                let qos = self.get_qos()?;
                qos.check_unsupported_policies()?;
                qos.is_consistent()?;
                if qos.autoenable_created_entities() {
                    self.enable_child_entities()?;
                }
                self.enable_rtps_entities()?;
                self.enabled.store(true, Ordering::SeqCst);
                Ok(())
            }

            fn get_instance_handle(&self) -> DdsResult<InstanceHandle> {
                self.is_deleted()?;
                Ok(InstanceHandle::from_guid(&self.guid))
            }
        }

        impl<$($impl_generics)*> Entity for $type
        where
            $($where_clause)*
        {
            type Qos = $qos_type;

            fn get_statuscondition(&self) -> DdsResult<StatusCondition<Self::Qos>> {
                self.is_deleted()?;
                let status_condition =
                    self.status_condition.lock().map_err(|e| DdsError::Error(e.to_string()))?;
                Ok(status_condition.clone())
            }

            fn set_qos(&self, qos: Self::Qos) -> DdsResult<()> {
                self.is_deleted()?;
                qos.check_unsupported_policies()?;
                qos.is_consistent()?;

                if self.is_enabled().is_ok() {
                    self.get_qos()?.check_immutable_change(&qos)?;
                }

                match self.qos.lock() {
                    Ok(mut prev_qos) => {
                        self.update_rtps_entity(&qos)?;
                        *prev_qos = qos;
                        Ok(())
                    }
                    Err(e) => Err(DdsError::Error(e.to_string())),
                }
            }

            fn get_qos(&self) -> DdsResult<Self::Qos> {
                self.is_deleted()?;
                match self.qos.lock() {
                    Ok(qos) => Ok(qos.clone()),
                    Err(e) => Err(DdsError::Error(e.to_string())),
                }
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
