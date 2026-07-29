use std::{
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

use crate::rtps::{
    common::rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
    entities::participant::Participant,
};

pub(crate) trait ParticipantAccessor {
    fn get_upgraded_participant(&self) -> RtpsResult<Arc<Participant>>;
}

pub(crate) trait UnicastThreadHandler {
    fn get_unicast_listening_handle(&self) -> RtpsResult<Arc<Mutex<Option<JoinHandle<()>>>>>;

    fn join_unicast_listening_thread(&self) -> RtpsResult<()> {
        let unicast_join_handle = self.get_unicast_listening_handle()?;

        if let Ok(mut handle_guard) = unicast_join_handle.lock() {
            if let Some(handle) = handle_guard.take() {
                handle.join().map_err(|_| RtpsError::new(RtpsErrorCode::ThreadJoinError, None))?;
            }
        }
        Ok(())
    }
}

pub(crate) trait MulticastThreadHandler {
    fn get_multicast_listening_handle(&self) -> RtpsResult<Arc<Mutex<Option<JoinHandle<()>>>>>;

    fn join_multicast_listening_thread(&self) -> RtpsResult<()> {
        let multicast_join_handle = self.get_multicast_listening_handle()?;

        if let Ok(mut handle_guard) = multicast_join_handle.lock() {
            if let Some(handle) = handle_guard.take() {
                handle.join().map_err(|_| RtpsError::new(RtpsErrorCode::ThreadJoinError, None))?;
            }
        }
        Ok(())
    }
}

pub(crate) trait JoinAllThread: UnicastThreadHandler + MulticastThreadHandler {
    fn join_all_listening_threads(&self) -> RtpsResult<()> {
        self.join_unicast_listening_thread()?;
        self.join_multicast_listening_thread()?;
        Ok(())
    }
}

macro_rules! impl_participant_accessor {
    ($type:ty) => {
        impl ParticipantAccessor for $type {
            fn get_upgraded_participant(&self) -> RtpsResult<Arc<Participant>> {
                self.participant.upgrade().ok_or_else(|| {
                    RtpsError::new(RtpsErrorCode::ArcUpgradeError, "Participant already dropped")
                })
            }
        }
    };
}
pub(crate) use impl_participant_accessor;

macro_rules! impl_unicast_thread_handler {
    ($type:ty) => {
        impl UnicastThreadHandler for $type {
            fn get_unicast_listening_handle(
                &self,
            ) -> RtpsResult<Arc<Mutex<Option<JoinHandle<()>>>>> {
                Ok(Arc::clone(&self.unicast_listening_handle))
            }
        }
    };
}
pub(crate) use impl_unicast_thread_handler;

macro_rules! impl_multicast_thread_handler {
    ($type:ty) => {
        impl MulticastThreadHandler for $type {
            fn get_multicast_listening_handle(
                &self,
            ) -> RtpsResult<Arc<Mutex<Option<JoinHandle<()>>>>> {
                Ok(Arc::clone(&self.multicast_listening_handle))
            }
        }
    };
}
pub(crate) use impl_multicast_thread_handler;
