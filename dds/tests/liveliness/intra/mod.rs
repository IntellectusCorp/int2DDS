// Intra-participant cells: writer and reader live on the same participant.
// Unmatch path goes through cleanup_remote_writer rather than SEDP dispose.

mod automatic;
mod manual_by_participant;
mod manual_by_topic;
