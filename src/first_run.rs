// SPDX-License-Identifier: GPL-3.0-only

//! The welcome at a new user's first login.
//!
//! An autostart entry runs `magnetar-accounts --first-run` at every login.
//! It opens the add page once — the first time, and only if there is no
//! account yet — and then never again, whether the person added one or
//! closed the window: a welcome that returns at every login until obeyed is
//! not a welcome.

use std::path::{Path, PathBuf};

/// What the first-run launch should do.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// Open the window on the add page.
    Welcome,
    /// Exit without a window.
    Nothing,
}

/// The file whose existence means the welcome has been shown.
#[must_use]
pub fn marker() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .unwrap_or_else(std::env::temp_dir)
        .join("magnetar-accounts")
        .join("first-run-done")
}

/// Decides, and records the decision so the next login does not ask again.
///
/// The marker is written before the window opens, not after it closes: a
/// session that ends with the window still open must not bring it back.
#[must_use]
pub fn decide(marker: &Path, has_accounts: bool) -> Decision {
    if marker.exists() {
        return Decision::Nothing;
    }
    if let Some(dir) = marker.parent()
        && let Err(why) = std::fs::create_dir_all(dir).and_then(|()| std::fs::write(marker, ""))
    {
        // Not being able to remember is a reason to stay quiet, not to
        // greet on every login.
        tracing::warn!(%why, marker = %marker.display(), "cannot record the first run");
        return Decision::Nothing;
    }
    if has_accounts {
        Decision::Nothing
    } else {
        Decision::Welcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_login_with_no_account_is_welcomed_once() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("state/first-run-done");

        assert_eq!(decide(&marker, false), Decision::Welcome);
        assert_eq!(
            decide(&marker, false),
            Decision::Nothing,
            "the welcome came back at the next login"
        );
    }

    #[test]
    fn someone_who_already_has_an_account_is_not_welcomed() {
        // An account added from Envelope before the first login finished, or
        // carried over in a restored home directory.
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("first-run-done");

        assert_eq!(decide(&marker, true), Decision::Nothing);
        assert!(marker.exists(), "the next login would have asked again");
    }

    #[test]
    fn a_marker_that_cannot_be_written_means_no_welcome() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("not-a-directory");
        std::fs::write(&blocker, "").unwrap();

        assert_eq!(
            decide(&blocker.join("first-run-done"), false),
            Decision::Nothing
        );
    }
}
