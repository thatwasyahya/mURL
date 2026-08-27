//! CLI subcommand implementations. Each module is one command family and
//! stays thin: parsing/validation/resolution/policy/dispatch all live in
//! `murl-core`; commands wire them to arguments, output, and exit codes.

pub mod cache_cmd;
pub mod create;
pub mod handler_cmd;
pub mod inspect;
pub mod keys;
pub mod name;
pub mod open;
pub mod os_cmd;
pub mod parse_cmd;
pub mod resolve;
pub mod trust_cmd;
pub mod validate;
