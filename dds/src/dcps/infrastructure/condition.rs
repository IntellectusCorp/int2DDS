//! Condition - Base trait for waitable conditions in DDS.
//!
//! The `Condition` trait is the base interface for all condition objects that can be
//! attached to a `WaitSet` and waited upon. Conditions represent boolean values that
//! can be triggered by various events in the DDS system.
//!
//! Concrete condition types include `StatusCondition`, `ReadCondition`, `QueryCondition`,
//! and `GuardCondition`. Applications use conditions with WaitSets to wait for specific
//! events or data availability without busy-polling.

use std::{any::Any, fmt::Debug, sync::Arc};

use crate::core::error::DdsResult;

#[allow(private_bounds)]
pub trait Condition: ConditionInternal {
    fn get_trigger_value(&self) -> DdsResult<bool>;
}

pub(crate) trait ConditionInternal: Debug {
    fn set_waitset_callback(&self, f: Option<Arc<dyn Fn() + Send + Sync>>);
    fn as_any(&self) -> &dyn Any;

    /// Stable identity of the underlying condition object.
    ///
    /// A condition reaches a WaitSet as `Arc<dyn Condition>`, and every
    /// `Into<Arc<dyn Condition + Send + Sync>>` allocates a *fresh* Arc, so
    /// pointer equality on the trait object says nothing about whether two
    /// handles denote the same condition. Cloning a condition shares its inner
    /// `Arc` fields instead, so the address of `waitset_callback` identifies
    /// the condition across handles, and does so without allocating.
    fn identity(&self) -> *const ();
}

macro_rules! impl_dds_condition_impl {
    ($type:ty, {$($impl_generics:tt)*}, {$($where_clause:tt)*}) => {
        impl<$($impl_generics)*> ConditionInternal for $type
        where
            $($where_clause)*
        {
            fn set_waitset_callback(&self, f: Option<Arc<dyn Fn() + Send + Sync>>) {
                match self.waitset_callback.lock() {
                    Ok(mut callback) => {
                        *callback = f;
                    }
                    Err(e) => {
                        log::error!("Failed to lock callback: {:?}", e);
                    }
                }
            }

            fn as_any(&self) -> &dyn Any {
                self
            }

            fn identity(&self) -> *const () {
                ::std::sync::Arc::as_ptr(&self.waitset_callback) as *const ()
            }
        }
        impl<$($impl_generics)*> $type
        where
            $($where_clause)*
        {
            /// Wrapper for Condition::get_trigger_value()
            #[inline]
            pub fn get_trigger_value(&self) -> DdsResult<bool> {
                <Self as Condition>::get_trigger_value(self)
            }
        }
    };
}

macro_rules! impl_dds_condition {
    // Existing version without generics
    ($type:ty) => {
        impl_dds_condition_impl!($type, {},  {});
    };

    // Version with generics - without where clause
    ($type:ty, $($generic:tt)*) => {
        impl_dds_condition_impl!($type, {$($generic)*}, {$($generic)*});
    };
}

pub(crate) use impl_dds_condition;
pub(crate) use impl_dds_condition_impl;
