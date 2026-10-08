// GROUP access_scope with ordered_access alone. Tests that need coherent_access as well live
// in ordered_coherent.rs.

pub(crate) use super::*;

#[path = "1-communication.rs"]
mod communication;
#[path = "4-connectivity.rs"]
mod connectivity;
#[path = "3-listener.rs"]
mod listener;
#[path = "2-non-group-subscriber.rs"]
mod non_group_subscriber;
