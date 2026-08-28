//! # murl-core
//!
//! Core library for **mURL (Multi-Resource Uniform Locator)** — an
//! experimental addressing primitive where one stable identifier resolves to
//! a *set* of heterogeneous resources (web URLs, local files, directories,
//! terminals, nested mURLs) instead of a single one:
//!
//! ```text
//! URL   :  identifier → resource
//! mURL  :  identifier → manifest → { resource, resource, resource, ... }
//! ```
//!
//! This crate contains everything that must be correct for mURL to be safe:
//!
//! * [`murl`] — the `murl://` parser (hostile input, fuzzed)
//! * [`manifest`] — the manifest model and validator (hostile input, fuzzed)
//! * [`canonical`] — MCF-1 canonical JSON, the byte form signatures cover
//! * [`resolver`] — name → manifest → flattened resource plan, with cycle
//!   detection, depth/count limits, identity binding, and cache fallback
//! * [`policy`] — SAFE/SENSITIVE/DANGEROUS classification + consent decisions
//! * [`trust`] — ed25519 signatures, key pinning, integrity hashes
//! * [`dispatch`] — argv construction and sequenced launching (no shell,
//!   ever), behind the [`dispatch::Launcher`] trait
//!
//! What this crate deliberately does **not** contain: network I/O
//! (embedders implement [`fetch::RemoteFetcher`]), process creation
//! (embedders implement [`dispatch::Launcher`]), and UI. The CLI in
//! `crates/murl-cli` wires those up.
//!
//! Specification: `spec/SPECIFICATION.md`. Status: **experimental** — the
//! format and this API can and will change before 1.0.

pub mod bundle;
pub mod cache;
pub mod canonical;
pub mod config;
pub mod dispatch;
pub mod error;
pub mod fetch;
pub mod grammar;
pub mod graph;
pub mod json;
pub mod kind;
pub mod limits;
pub mod manifest;
pub mod murl;
pub mod policy;
pub mod resolver;
pub mod time;
pub mod trust;

pub use bundle::{Bundle, BundleEntry};
pub use config::{HandlersFile, UserConfig};
pub use error::{Error, Result};
pub use kind::Kind;
pub use limits::Limits;
pub use manifest::{Manifest, ManifestDoc, ResourceDoc, ValidationReport};
pub use murl::{Authority, Murl, VersionTag};
pub use policy::{classify, ConsentMode, Decision, Policy, Tier};
pub use resolver::{Origin, PlannedResource, Resolution, ResolvedNode, Resolver};
pub use trust::{Keypair, TrustStatus, TrustStore};
