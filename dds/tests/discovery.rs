// Endpoint discovery integration tests (SEDP late-join, match race, builtin keying).

mod common;

#[path = "discovery/late_joiner.rs"]
mod late_joiner;

#[path = "discovery/match_race.rs"]
mod match_race;

#[path = "discovery/concurrent.rs"]
mod concurrent;
