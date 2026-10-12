// SPDX-License-Identifier: GPL-3.0-only

//! The command-line half: what is there, and bringing in what GNOME Online
//! Accounts holds. For scripts — a first-boot step, a support session — and
//! for anyone who would rather not open a window to find out.

use std::process::ExitCode;

use cosmic_pim_accounts::{Account, AccountStore, Registry};
use cosmic_pim_auth::OnlineAccounts;
use cosmic_pim_sync::setup::{self, Services};

/// Prints every account and what it brings. Never a secret.
#[must_use]
pub fn list() -> ExitCode {
    let store = match AccountStore::open_default() {
        Ok(store) => store,
        Err(why) => {
            eprintln!("magnetar-accounts: the account list could not be read: {why}");
            return ExitCode::FAILURE;
        }
    };
    let registry = Registry::load();
    if store.accounts().is_empty() {
        println!("No accounts.");
    }
    for account in store.accounts() {
        println!("{}", line(account, &registry));
    }
    ExitCode::SUCCESS
}

/// One account, as `list` prints it.
fn line(account: &Account, registry: &Registry) -> String {
    let services = describe(setup::services_of(account, registry));
    let how = if account.online_account().is_some() {
        "Online Accounts"
    } else if account.is_oauth() {
        "sign-in"
    } else {
        "password"
    };
    let provider = account
        .provider
        .as_deref()
        .and_then(|id| registry.get(id))
        .map_or_else(String::new, |provider| format!(" · {}", provider.name));
    let off = if account.enabled { "" } else { " · off" };
    format!(
        "{}\t{}{provider} · {how} · {services}{off}",
        account.display_name, account.username
    )
}

/// "mail, calendar, contacts", or what of it there is.
#[must_use]
pub fn describe(services: Services) -> String {
    let names: Vec<&str> = [
        (services.mail, "mail"),
        (services.calendar, "calendar"),
        (services.contacts, "contacts"),
    ]
    .into_iter()
    .filter_map(|(on, name)| on.then_some(name))
    .collect();
    if names.is_empty() {
        "nothing to sync".to_owned()
    } else {
        names.join(", ")
    }
}

/// Links the accounts GNOME Online Accounts holds, and forgets the ones it
/// no longer does.
#[must_use]
pub fn adopt() -> ExitCode {
    adopt_from(
        OnlineAccounts::connect(),
        AccountStore::open_default,
        &Registry::load(),
    )
}

/// [`adopt`], given the answer to "is Online Accounts there" and how to open
/// the list. The list is opened only when there is something to bring in.
fn adopt_from(
    service: Result<Option<OnlineAccounts>, cosmic_pim_auth::Error>,
    open: impl FnOnce() -> Result<AccountStore, cosmic_pim_accounts::Error>,
    registry: &Registry,
) -> ExitCode {
    let service = match service {
        Ok(Some(service)) => service,
        Ok(None) => {
            println!("Online Accounts is not running; nothing to bring in.");
            return ExitCode::SUCCESS;
        }
        Err(why) => {
            eprintln!("magnetar-accounts: {why}");
            return ExitCode::FAILURE;
        }
    };
    let mut store = match open() {
        Ok(store) => store,
        Err(why) => {
            eprintln!("magnetar-accounts: the account list could not be read: {why}");
            return ExitCode::FAILURE;
        }
    };

    match setup::adopt_online_accounts(&mut store, registry, &service) {
        Ok(adopted) => {
            for id in &adopted.linked {
                println!("linked {id}");
            }
            for id in &adopted.removed {
                println!("removed {id}: no longer in Online Accounts");
            }
            for (id, why) in &adopted.failed {
                eprintln!("could not link {id}: {why}");
            }
            if adopted.failed.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(why) => {
            eprintln!("magnetar-accounts: {why}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeAccount, PrivateBus};

    #[test]
    fn a_listed_account_says_how_it_signs_in_and_what_it_brings() {
        let registry = Registry::load_from(std::path::Path::new("/nonexistent"));
        let account = registry
            .get("fastmail")
            .unwrap()
            .account_for("ada@fastmail.com");

        let printed = line(&account, &registry);

        assert!(printed.contains("ada@fastmail.com"), "{printed}");
        assert!(printed.contains("Fastmail"), "{printed}");
        assert!(printed.contains("password"), "{printed}");
        assert!(printed.contains("mail, calendar, contacts"), "{printed}");
    }

    fn built_in() -> Registry {
        Registry::load_from(std::path::Path::new("/nonexistent"))
    }

    fn store_at(dir: &std::path::Path) -> Result<AccountStore, cosmic_pim_accounts::Error> {
        let secrets =
            cosmic_pim_accounts::SecretStore::open_envelope_only("magnetar-accounts-test", dir);
        AccountStore::open(&dir.join("accounts.toml"), secrets)
    }

    #[test]
    fn adopting_without_online_accounts_succeeds_and_leaves_the_list_alone() {
        let bus = PrivateBus::start(true);
        let dir = tempfile::tempdir().unwrap();

        let code = adopt_from(
            OnlineAccounts::on(bus.connect()),
            || store_at(dir.path()),
            &built_in(),
        );

        assert_eq!(code, ExitCode::SUCCESS);
        assert!(!dir.path().join("accounts.toml").exists());
    }

    #[test]
    fn adopting_links_what_online_accounts_holds() {
        let bus = PrivateBus::start(true);
        let _daemon = bus.serve(&[FakeAccount::google("account_1", "ada@gmail.com")]);
        let dir = tempfile::tempdir().unwrap();

        let code = adopt_from(
            OnlineAccounts::on(bus.connect()),
            || store_at(dir.path()),
            &built_in(),
        );

        assert_eq!(code, ExitCode::SUCCESS);
        let store = store_at(dir.path()).unwrap();
        let linked: Vec<_> = store
            .accounts()
            .iter()
            .map(|account| (account.username.as_str(), account.online_account()))
            .collect();
        assert_eq!(linked, [("ada@gmail.com", Some("account_1"))]);
    }

    #[test]
    fn an_account_with_nothing_to_sync_says_so() {
        assert_eq!(describe(Services::default()), "nothing to sync");
    }
}
