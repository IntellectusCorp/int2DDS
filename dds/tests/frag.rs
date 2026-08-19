// Fragmentation (DATA_FRAG) integration tests.

mod common;

#[path = "frag/basic.rs"]
mod basic;

#[path = "frag/small_payload.rs"]
mod small_payload;

#[path = "frag/window.rs"]
mod window;

#[path = "frag/round_latency.rs"]
mod round_latency;

#[path = "frag/repair_progress.rs"]
mod repair_progress;

#[path = "frag/packed_submessage.rs"]
mod packed_submessage;

#[path = "frag/batched_two_readers.rs"]
mod batched_two_readers;
