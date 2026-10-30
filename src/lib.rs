//! plied: a git-tracked work record for teams of agents and people.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod access;
pub mod baseline;
pub mod check;
pub mod config;
pub mod doc;
pub mod error;
pub mod fsx;
pub mod globpat;
pub mod literature;
pub mod gitguard;
pub mod hook;
pub mod index;
pub mod messages;
pub mod notes;
pub mod out;
pub mod pdf;
pub mod query;
pub mod records;
pub mod repo;
pub mod schema;
pub mod session;
pub mod skill;
mod summary;
pub mod review;
pub mod textmatch;
pub mod val;
pub mod views;
pub mod writer;
pub mod yamlish;
