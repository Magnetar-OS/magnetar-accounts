// SPDX-License-Identifier: GPL-3.0-only

//! Accounts — the one place on the desktop where mail, calendar and contacts
//! accounts are added.
//!
//! An account is the suite's, not an application's: one list in
//! `$XDG_CONFIG_HOME/cosmic-pim/accounts.toml`, read by Envelope, Slate and
//! Circle alike. This window adds to it, and the applications hand off to it
//! from their first-run pages. Everything below the window — which provider an
//! address belongs to, how it signs in, trying a password before keeping it,
//! the browser sign-in, GNOME Online Accounts — is `cosmic_pim_sync::setup`.
//!
//! # Command line
//!
//! ```text
//! magnetar-accounts                 the window
//! magnetar-accounts --add           the window, on the add page
//! magnetar-accounts --add --for=mail
//!                                   ...saying which application asked, and why
//! magnetar-accounts list            every account and what it brings
//! magnetar-accounts adopt           link the accounts GNOME Online Accounts holds
//! magnetar-accounts --first-run     the welcome, once, at a new user's first login
//! ```

pub mod app;
pub mod cli;
pub mod first_run;
pub mod i18n;
#[cfg(test)]
mod testing;

use std::process::ExitCode;

/// What the window was asked to open on.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Request {
    /// Open on the add page rather than the list.
    #[serde(default)]
    pub add: bool,
    /// The service the asking application needs, for saying so.
    #[serde(default)]
    pub purpose: Option<Purpose>,
}

/// The service an application needs an account for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Purpose {
    Mail,
    Calendar,
    Contacts,
}

impl std::str::FromStr for Purpose {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "mail" => Ok(Self::Mail),
            "calendar" => Ok(Self::Calendar),
            "contacts" => Ok(Self::Contacts),
            other => Err(format!(
                "unknown purpose “{other}”: expected mail, calendar or contacts"
            )),
        }
    }
}

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Window(Request),
    List,
    Adopt,
    /// The welcome at a new user's first login — from the autostart entry.
    FirstRun,
}

/// Reads the command line, without the program name.
///
/// # Errors
///
/// An argument that means nothing, said in words.
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut request = Request::default();
    let mut command = None;
    for arg in args {
        match arg.as_str() {
            "list" if command.is_none() => command = Some(Command::List),
            "adopt" if command.is_none() => command = Some(Command::Adopt),
            "--first-run" if command.is_none() => command = Some(Command::FirstRun),
            "--add" => request.add = true,
            other => match other.strip_prefix("--for=") {
                Some(purpose) => {
                    request.purpose = Some(purpose.parse()?);
                    request.add = true;
                }
                None => return Err(format!("unknown argument “{other}”")),
            },
        }
    }
    match command {
        None => Ok(Command::Window(request)),
        // `list --add` would otherwise drop the `--add` without a word.
        Some(_) if request != Request::default() => {
            Err("--add and --for= open the window, and cannot go with a command".to_owned())
        }
        Some(command) => Ok(command),
    }
}

/// Runs whatever the command line asked for.
#[must_use]
pub fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "magnetar_accounts=warn".into()),
        )
        .init();

    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();
    i18n::init(&requested_languages);

    match parse(std::env::args().skip(1)) {
        Ok(Command::List) => cli::list(),
        Ok(Command::Adopt) => cli::adopt(),
        Ok(Command::FirstRun) => match first_run::decide(&first_run::marker(), has_accounts()) {
            first_run::Decision::Welcome => match run(Request {
                add: true,
                purpose: None,
            }) {
                Ok(()) => ExitCode::SUCCESS,
                Err(why) => {
                    eprintln!("magnetar-accounts: {why}");
                    ExitCode::FAILURE
                }
            },
            first_run::Decision::Nothing => ExitCode::SUCCESS,
        },
        Ok(Command::Window(request)) => match run(request) {
            Ok(()) => ExitCode::SUCCESS,
            Err(why) => {
                eprintln!("magnetar-accounts: {why}");
                ExitCode::FAILURE
            }
        },
        Err(why) => {
            eprintln!("magnetar-accounts: {why}");
            ExitCode::from(2)
        }
    }
}

/// Whether the account list has anything in it. Metadata only: the keychain
/// is not asked, so a locked one does not hold up a login.
fn has_accounts() -> bool {
    std::fs::read_to_string(cosmic_pim_accounts::account::default_config_path())
        .is_ok_and(|text| text.contains("[[account]]"))
}

/// Opens the window.
///
/// # Errors
///
/// When the windowing system refuses to start.
pub fn run(request: Request) -> cosmic::iced::Result {
    let settings = cosmic::app::Settings::default()
        .size(cosmic::iced::Size::new(720.0, 680.0))
        .size_limits(
            cosmic::iced::Limits::NONE
                .min_width(360.0)
                .min_height(420.0),
        );

    // `run_single_instance`: an application's first-run page may ask for
    // this window while it is already open, and should raise it — on the add
    // page — rather than open a second one.
    cosmic::app::run_single_instance::<app::AppModel>(settings, app::Flags::new(request))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command, String> {
        super::parse(args.iter().map(ToString::to_string))
    }

    #[test]
    fn no_arguments_opens_the_list() {
        assert_eq!(parse(&[]), Ok(Command::Window(Request::default())));
    }

    #[test]
    fn a_purpose_opens_the_add_page_for_it() {
        assert_eq!(
            parse(&["--for=calendar"]),
            Ok(Command::Window(Request {
                add: true,
                purpose: Some(Purpose::Calendar),
            }))
        );
    }

    #[test]
    fn the_command_line_tools_are_words() {
        assert_eq!(parse(&["list"]), Ok(Command::List));
        assert_eq!(parse(&["adopt"]), Ok(Command::Adopt));
        assert_eq!(parse(&["--first-run"]), Ok(Command::FirstRun));
    }

    #[test]
    fn nonsense_is_refused_rather_than_ignored() {
        assert!(parse(&["--frobnicate"]).is_err());
        assert!(parse(&["--for=fax"]).is_err());
        assert!(parse(&["--for="]).is_err());
        assert!(parse(&["list", "adopt"]).is_err());
        assert!(parse(&["list", "--add"]).is_err());
        assert!(parse(&["--for=mail", "--first-run"]).is_err());
    }

    #[test]
    fn each_application_asks_for_its_own_service() {
        // What Envelope, Slate and Circle pass from their first-run pages.
        for (word, purpose) in [
            ("mail", Purpose::Mail),
            ("calendar", Purpose::Calendar),
            ("contacts", Purpose::Contacts),
        ] {
            assert_eq!(
                parse(&[&format!("--for={word}")]),
                Ok(Command::Window(Request {
                    add: true,
                    purpose: Some(purpose),
                }))
            );
        }
        assert_eq!(
            parse(&["--add"]),
            Ok(Command::Window(Request {
                add: true,
                purpose: None,
            }))
        );
    }
}
