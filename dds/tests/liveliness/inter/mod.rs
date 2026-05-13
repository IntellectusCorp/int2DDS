// Inter-participant cells: writer and reader live on separate participants
// (same domain). Match goes through SEDP discovery; unmatch goes through
// SEDP dispose, not the in-process cleanup path that intra cells exercise.
// Timeouts are slacker than intra to absorb discovery cadence.

mod automatic;
mod manual_by_participant;
mod manual_by_topic;
