//! Shared password rate-limit messaging.

use std::time::Duration;

/// User-facing message when a password key is temporarily locked.
pub fn lockout_message(lockout_duration: Duration) -> String {
    let mins = lockout_duration.as_secs() / 60;
    format!("Too many failed attempts. Try again in about {mins} minutes.")
}

/// User-facing message for import/export password lockout (slightly different wording).
pub fn import_export_lockout_message(lockout_duration: Duration) -> String {
    let mins = lockout_duration.as_secs() / 60;
    format!("Too many failed password attempts. Try again in about {mins} minutes.")
}

