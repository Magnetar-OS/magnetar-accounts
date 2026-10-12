// SPDX-License-Identifier: GPL-3.0-only

//! Keeping the account list in step with GNOME Online Accounts, with no
//! window open.
//!
//! `magnetar-accounts watch` runs from an autostart entry at every login. It
//! links what Online Accounts holds once, then again whenever Online Accounts
//! adds or removes an account, so an account added in GNOME's own window
//! reaches Envelope, Slate and Circle without anyone opening this one.
//!
//! # When it does nothing
//!
//! Online Accounts is usually started on demand: at login it may not be
//! running yet, and the first application to ask for it starts it. So "not
//! running" is not "not there". The watcher asks the bus whether it *could*
//! start the service; where it could not — Online Accounts is not installed —
//! it exits at once, having touched nothing. Where it could, it waits for the
//! service to appear, and never starts it itself: a login is no reason to.
//!
//! # What it watches
//!
//! The daemon's object manager: `InterfacesAdded` and `InterfacesRemoved` on
//! `/org/gnome/OnlineAccounts` are an account arriving or leaving, and the
//! bus's `NameOwnerChanged` for `org.gnome.OnlineAccounts` is the daemon
//! starting or stopping. Signals arrive in bursts — one account is two
//! interfaces, so two signals — and each burst is answered with one pass of
//! `setup::adopt_online_accounts`, which links what is new and forgets what
//! Online Accounts no longer holds. The pass reads the list afresh each time,
//! so what the window or another application wrote in between is kept.

use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Duration;

use cosmic_pim_accounts::{AccountStore, Registry};
use cosmic_pim_auth::OnlineAccounts;
use cosmic_pim_sync::setup::{self, Adopted, SetupError};
use zbus::MatchRule;
use zbus::blocking::fdo::DBusProxy;
use zbus::blocking::{Connection, MessageIterator};
use zbus::names::BusName;

const BUS: &str = "org.gnome.OnlineAccounts";
const ROOT: &str = "/org/gnome/OnlineAccounts";
const OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";

/// How long a burst of signals is let run before it is answered.
const SETTLE: Duration = Duration::from_millis(400);

/// Why [`watch`] returned.
#[derive(Debug, PartialEq, Eq)]
pub enum Ended {
    /// Online Accounts is not installed: nothing was done.
    NotInstalled,
    /// Online Accounts stopped, and nothing on the bus can start it again.
    Gone,
    /// The session bus closed: the session is over.
    SessionClosed,
}

/// Runs the watcher on the session bus until there is nothing left to watch.
#[must_use]
pub fn run() -> ExitCode {
    let connection = match Connection::session() {
        Ok(connection) => connection,
        Err(why) => {
            eprintln!("magnetar-accounts: the session bus could not be reached: {why}");
            return ExitCode::FAILURE;
        }
    };
    let registry = Registry::load();
    let ended = watch(&connection, |service| match AccountStore::open_default() {
        Ok(mut store) => report(setup::adopt_online_accounts(&mut store, &registry, service)),
        Err(why) => tracing::warn!(%why, "the account list could not be read"),
    });
    match ended {
        Ok(ended) => {
            tracing::info!(?ended, "nothing left to watch");
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("magnetar-accounts: watching Online Accounts failed: {why}");
            ExitCode::FAILURE
        }
    }
}

/// Logs what one pass changed. Nobody is looking at a terminal: this is the
/// session's journal.
fn report(result: Result<Adopted, SetupError>) {
    match result {
        Ok(adopted) => {
            for id in &adopted.linked {
                tracing::info!(id, "linked from Online Accounts");
            }
            for id in &adopted.removed {
                tracing::info!(id, "removed: no longer in Online Accounts");
            }
            for (id, why) in &adopted.failed {
                tracing::warn!(id, why, "an Online Accounts account could not be linked");
            }
        }
        Err(why) => tracing::warn!(%why, "Online Accounts could not be brought in"),
    }
}

/// Watches Online Accounts on `connection`, calling `adopt` once when it is
/// first seen and once after every burst of changes, until it is gone for
/// good or the bus closes.
///
/// # Errors
///
/// When the bus refuses the subscriptions or a question about who owns a
/// name.
pub fn watch(
    connection: &Connection,
    mut adopt: impl FnMut(&OnlineAccounts),
) -> zbus::Result<Ended> {
    let bus = DBusProxy::new(connection)?;
    let name = BusName::try_from(BUS)?;
    let installed = || -> zbus::Result<bool> {
        Ok(bus
            .list_activatable_names()?
            .iter()
            .any(|activatable| activatable.as_str() == BUS))
    };

    // Subscribed before the first look, so a change between the look and the
    // subscription is not missed.
    let changes = subscribe(connection)?;

    if !bus.name_has_owner(name.clone())? && !installed()? {
        return Ok(Ended::NotInstalled);
    }

    loop {
        if bus.name_has_owner(name.clone())? {
            if let Some(service) = OnlineAccounts::on(connection.clone())
                .map_err(|why| zbus::Error::Failure(why.to_string()))?
            {
                adopt(&service);
            }
        } else if !installed()? {
            return Ok(Ended::Gone);
        }

        // Wait for the next change, then let the burst it starts finish.
        if changes.recv().is_err() {
            return Ok(Ended::SessionClosed);
        }
        while changes.recv_timeout(SETTLE).is_ok() {}
    }
}

/// The signals that mean the accounts may have changed, as one channel. Each
/// subscription is drained on a thread of its own; the channel closes when
/// both have ended, which is when the bus has gone.
fn subscribe(connection: &Connection) -> zbus::Result<mpsc::Receiver<()>> {
    let accounts = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(OBJECT_MANAGER)?
        .path(ROOT)?
        .build();
    let owner = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.DBus")?
        .interface("org.freedesktop.DBus")?
        .member("NameOwnerChanged")?
        .add_arg(BUS)?
        .build();

    let (sender, receiver) = mpsc::channel();
    for rule in [accounts, owner] {
        let messages = MessageIterator::for_match_rule(rule, connection, None)?;
        let sender = sender.clone();
        std::thread::Builder::new()
            .name("online-accounts-signals".to_owned())
            .spawn(move || {
                for message in messages {
                    if message.is_err() || sender.send(()).is_err() {
                        break;
                    }
                }
            })
            .map_err(|why| zbus::Error::Failure(why.to_string()))?;
    }
    Ok(receiver)
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Instant;

    use cosmic_pim_accounts::SecretStore;

    use super::*;
    use crate::testing::{FakeAccount, PrivateBus};

    fn store_at(dir: &Path) -> AccountStore {
        let secrets = SecretStore::open_envelope_only("magnetar-accounts-test", dir);
        AccountStore::open(&dir.join("accounts.toml"), secrets).unwrap()
    }

    /// Runs the watcher on `bus` in the background, adopting into a store in
    /// `dir`, as `run` does with the real one.
    fn start(bus: &PrivateBus, dir: &Path) -> std::thread::JoinHandle<zbus::Result<Ended>> {
        let connection = bus.connect();
        let dir = dir.to_owned();
        std::thread::spawn(move || {
            let registry = Registry::load_from(Path::new("/nonexistent"));
            watch(&connection, |service| {
                let mut store = store_at(&dir);
                report(setup::adopt_online_accounts(&mut store, &registry, service));
            })
        })
    }

    /// The Online Accounts ids the list in `dir` links, once they are
    /// `expected` — or what they were when time ran out.
    fn linked_becomes(dir: &Path, expected: &[&str]) -> Vec<String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let mut linked: Vec<String> = store_at(dir)
                .accounts()
                .iter()
                .filter_map(|account| account.online_account().map(str::to_owned))
                .collect();
            linked.sort();
            if linked == expected || Instant::now() > deadline {
                return linked;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn finishes(watcher: std::thread::JoinHandle<zbus::Result<Ended>>) -> zbus::Result<Ended> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !watcher.is_finished() {
            assert!(Instant::now() < deadline, "the watcher did not stop");
            std::thread::sleep(Duration::from_millis(50));
        }
        watcher.join().expect("the watcher panicked")
    }

    #[test]
    fn without_online_accounts_installed_it_does_nothing_and_stops() {
        let bus = PrivateBus::start(false);
        let mut asked = false;

        let ended = watch(&bus.connect(), |_| asked = true).unwrap();

        assert_eq!(ended, Ended::NotInstalled);
        assert!(!asked, "with nothing to link, the list must not be touched");
    }

    #[test]
    fn accounts_added_and_removed_in_online_accounts_follow_into_the_list() {
        // Installed but not yet running, as at a login: the watcher waits for
        // the daemon rather than giving up or starting it.
        let bus = PrivateBus::start(true);
        let dir = tempfile::tempdir().unwrap();
        let watcher = start(&bus, dir.path());
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            !watcher.is_finished(),
            "an installed service is worth waiting for"
        );

        let daemon = bus.serve(&[FakeAccount::google("account_1", "ada@gmail.com")]);
        assert_eq!(linked_becomes(dir.path(), &["account_1"]), ["account_1"]);

        daemon.add(&FakeAccount::google("account_2", "grace@gmail.com"));
        assert_eq!(
            linked_becomes(dir.path(), &["account_1", "account_2"]),
            ["account_1", "account_2"]
        );

        daemon.remove("account_1");
        assert_eq!(linked_becomes(dir.path(), &["account_2"]), ["account_2"]);

        // Online Accounts stopping is not the end while it can start again;
        // the session ending is.
        drop(daemon);
        std::thread::sleep(SETTLE * 2);
        assert!(!watcher.is_finished());
        assert_eq!(
            linked_becomes(dir.path(), &["account_2"]),
            ["account_2"],
            "a daemon that stopped is not a daemon whose accounts were removed"
        );
        drop(bus);
        assert_eq!(finishes(watcher).unwrap(), Ended::SessionClosed);
    }

    #[test]
    fn a_daemon_that_cannot_come_back_ends_the_watch() {
        let bus = PrivateBus::start(false);
        let dir = tempfile::tempdir().unwrap();
        let daemon = bus.serve(&[FakeAccount::google("account_1", "ada@gmail.com")]);
        let watcher = start(&bus, dir.path());
        assert_eq!(linked_becomes(dir.path(), &["account_1"]), ["account_1"]);

        drop(daemon);

        assert_eq!(finishes(watcher).unwrap(), Ended::Gone);
        assert_eq!(linked_becomes(dir.path(), &["account_1"]), ["account_1"]);
    }
}
