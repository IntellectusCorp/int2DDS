// Liveliness integration tests, organised as a (topology × qos × transition)
// matrix. See tests/liveliness/intra/mod.rs and tests/liveliness/inter/mod.rs
// for layout. Test paths read e.g. `intra::manual_by_topic::lost`.

mod common;

#[path = "liveliness/helpers.rs"]
mod helpers;

#[path = "liveliness/intra/mod.rs"]
mod intra;

#[path = "liveliness/inter/mod.rs"]
mod inter;
