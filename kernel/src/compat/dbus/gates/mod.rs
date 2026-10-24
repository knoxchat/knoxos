//! GATE_DBUS self-tests for the in-kernel message bus.
//!
//! Each test opens a system-bus connection, sends a well-known method (or
//! match rule), and checks the reply. Boot `init()` and `security_tests`
//! call the same `*_self_test` functions; serial markers stay
//! `GATE_DBUS <name>`.
//!
//! Split by bus surface:
//!   names          — Hello, RequestName, ReleaseName, ListNames, ownership
//!   identity       — pid/uid/creds and LSM labels
//!   match_rules    — AddMatch, RemoveMatch, BecomeMonitor, name signals
//!   admin          — GetId, activation env, ReloadConfig, GetConfigFile
//!   introspection  — Peer, Introspectable, Properties, ObjectManager
//!   stats          — Debug.Stats GetStats / GetAllMatchRules / GetConnectionStats
mod admin;
mod identity;
mod introspection;
mod match_rules;
mod names;
mod stats;

pub use admin::*;
pub use identity::*;
pub use introspection::*;
pub use match_rules::*;
pub use names::*;
pub use stats::*;
