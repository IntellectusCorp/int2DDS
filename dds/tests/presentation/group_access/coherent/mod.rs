// GROUP access_scope with coherent_access alone. Tests that need ordered_access as well live
// in ordered_coherent.rs.

pub(crate) use super::*;

#[path = "2-atomicity.rs"]
mod atomicity;
#[path = "1-communication.rs"]
mod communication;
#[path = "5-connectivity.rs"]
mod connectivity;
#[path = "4-listener.rs"]
mod listener;
#[path = "3-non-group-subscriber.rs"]
mod non_group_subscriber;
