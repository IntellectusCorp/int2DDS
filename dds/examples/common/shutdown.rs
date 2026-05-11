#![allow(dead_code)]

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use int2dds::domain::{
    domain_participant::DomainParticipant, domain_participant_factory::DomainParticipantFactory,
};
use log::info;

pub struct Shutdown {
    flag: Mutex<bool>,
    cv: Condvar,
}

impl Shutdown {
    pub fn install() -> Arc<Self> {
        let me = Arc::new(Self { flag: Mutex::new(false), cv: Condvar::new() });
        let me_clone = Arc::clone(&me);
        let _ = ctrlc::set_handler(move || me_clone.stop());
        me
    }

    pub fn stop(&self) {
        *self.flag.lock().unwrap() = true;
        self.cv.notify_all();
    }

    pub fn is_stopped(&self) -> bool {
        *self.flag.lock().unwrap()
    }

    // Block up to `dur`, returning true once stop has been signalled.
    pub fn wait_timeout(&self, dur: Duration) -> bool {
        let guard = self.flag.lock().unwrap();
        let (guard, _) = self.cv.wait_timeout_while(guard, dur, |stopped| !*stopped).unwrap();
        *guard
    }

    // Block until stop is signalled.
    pub fn wait(&self) {
        let guard = self.flag.lock().unwrap();
        let _guard = self.cv.wait_while(guard, |stopped| !*stopped).unwrap();
    }
}

pub fn cleanup_participant(participant: DomainParticipant) {
    info!("Received shutdown signal, cleaning up resources");

    if let Err(e) = participant.delete_contained_entities() {
        eprintln!("delete_contained_entities failed: {:?}", e);
    }
    if let Err(e) = DomainParticipantFactory::get_instance().delete_participant(participant) {
        eprintln!("delete_participant failed: {:?}", e);
    }
}
