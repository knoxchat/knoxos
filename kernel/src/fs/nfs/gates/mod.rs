//! GATE_NFS self-tests for the in-kernel NFSv4 compound client.
//!
//! Each test builds a loopback `NfsClient`, runs a compound, and checks
//! status or bytes. Boot `nfs::init()` and `vfs_tests` call the same
//! `*_self_test` functions; serial markers stay `GATE_NFS <name>`.
//!
//! Split by NFS4 surface:
//!   io       — READ, WRITE, COMMIT, SETATTR, ACCESS, CREATE/REMOVE/RENAME/LINK/READDIR, ALLOCATE, COPY, DEALLOCATE
//!   fh       — GETFH, PUTFH, LOOKUPP, RESTOREFH, PUTPUBFH
//!   verify   — VERIFY, NVERIFY
//!   state    — SETCLIENTID_CONFIRM, CLOSE, RENEW
//!   lock     — LOCK, LOCKU, LOCKT, RELEASE_LOCKOWNER
//!   open     — OPEN_CONFIRM, OPEN_DOWNGRADE
//!   attr     — SECINFO, OPENATTR, SECINFO_NO_NAME
//!   deleg    — DELEGPURGE, DELEGRETURN, GET_DIR_DELEGATION, WANT_DELEGATION
//!   session  — EXCHANGE_ID, CREATE_SESSION, DESTROY_SESSION, DESTROY_CLIENTID, SEQUENCE, RECLAIM_COMPLETE, BIND_CONN_TO_SESSION, FREE_STATEID, BACKCHANNEL_CTL, TEST_STATEID
mod attr;
mod deleg;
mod fh;
mod io;
mod lock;
mod open;
mod session;
mod state;
mod verify;

pub use attr::*;
pub use deleg::*;
pub use fh::*;
pub use io::*;
pub use lock::*;
pub use open::*;
pub use session::*;
pub use state::*;
pub use verify::*;
