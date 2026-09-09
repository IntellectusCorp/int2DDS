// Liveliness integration tests, organised as a (topology × qos × transition)
// matrix. See tests/liveliness/intra/mod.rs and tests/liveliness/inter/mod.rs
// for layout. Test paths read e.g. `intra::manual_by_topic::lost`.

#[path = "../common/mod.rs"]
mod common;

mod helpers;
mod inter;
mod intra;
