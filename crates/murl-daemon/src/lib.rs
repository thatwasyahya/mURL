//! # murl-daemon
//!
//! A resident mURL resolver. Its reason to exist is a **persistent consent
//! surface** — a GUI dialog instead of a terminal that may not be there;
//! the warm cache, single-instance activation, and status endpoint are
//! consequences of being resident, not justifications for it.
//!
//! Non-negotiable properties (see `docs/daemon.md` for the IPC threat
//! model, entries D-1 … D-7):
//!
//! * **The daemon is never required.** Clients fall back to in-process
//!   resolution, fail-closed, when it is absent or looks wrong.
//! * **No endpoint dispatches without consent.** `activate` asks the
//!   daemon's own UI; a client cannot pre-approve on the user's behalf.
//! * **User-private socket**, 0600 inside a 0700 directory, ownership
//!   verified by both sides.
//! * **Closed protocol.** Exact version match, hard line cap, strict
//!   duplicate-rejecting JSON, unknown request types refused.

pub mod client;
pub mod consent_ui;
pub mod dialog_ui;
pub mod protocol;
pub mod server;
pub mod service;
pub mod socket;
pub mod terminal_ui;

pub use consent_ui::{ConsentItem, ConsentRequest, ConsentUi, DenyAllUi};
pub use dialog_ui::DialogUi;
pub use protocol::{Request, Response, PROTOCOL_VERSION};
pub use server::{handle_request, serve_connection, Context};
