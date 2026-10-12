# Accounts

Where mail, calendar and contacts accounts are added on the COSMIC desktop.
Add one once, and [Envelope](https://github.com/Magnetar-OS/envelope),
[Slate](https://github.com/Magnetar-OS/slate) and
[Circle](https://github.com/Magnetar-OS/circle) all use it.

Type an address. Accounts works out which provider it belongs to, what the
account will bring — mail, calendars, contacts — and which ways of signing in
can work on this computer, best first:

- **The provider's own sign-in**, in your browser, for Google and Microsoft
  where this desktop has a client registration for them.
- **GNOME Online Accounts**, where it is running: it signs Google and
  Microsoft accounts in under GNOME's registration and keeps them signed in.
  Accounts added there show up here, and in the applications, by themselves.
- **A password** — for most providers an app password. It is tried against
  the server before it is kept, so a mistyped one is caught on the spot rather
  than an hour later as a sync error. An address no provider claims has its
  servers found from the address.

Built on the suite's substrate, [cosmic-pim](https://github.com/Magnetar-OS/cosmic-pim):
the account list is `$XDG_CONFIG_HOME/cosmic-pim/accounts.toml`, secrets go
to the system keyring, and everything below the window is
`cosmic_pim_sync::setup`. This repository is the window and the command line.

## Use

```text
magnetar-accounts                   the window
magnetar-accounts --add             the window, on the add page
magnetar-accounts --for=mail        ...saying which application asked (mail, calendar, contacts)
magnetar-accounts list              every account and what it brings
magnetar-accounts adopt             link what GNOME Online Accounts holds
magnetar-accounts --first-run       the welcome at a new user's first login
```

An application with no account opens `magnetar-accounts --for=<service>` from
its first-run page. A second launch raises the window already open.

A welcome ships as an autostart entry in `/etc/xdg/autostart`:
`--first-run` opens the add page once, at a new user's first login, and only
if there is no account yet. It never comes back, whether an account was
added or the window was closed.

GNOME Online Accounts (`gnome-online-accounts`, and `gnome-online-accounts-gtk`
for its window outside GNOME) is optional: without it, every other way in
works the same.

## Build

```sh
just build-release
just run-sandboxed      # a scratch account list, the keychain left alone
just check-all          # metadata, fmt, clippy, tests
```

The tests start a private `dbus-daemon` with a stand-in Online Accounts
daemon on it, so `dbus-daemon` must be installed; the session's own bus and
accounts are never touched.

## Status

New. What works: adding by password with a login check first, the browser
sign-in, GNOME Online Accounts, removing, the command line. What does not yet:

- An address whose servers cannot be found cannot be added here; Envelope's
  server form is the way for now.
- Editing an account — its name, its servers — happens in the applications.
- Which of mail, calendar and contacts an account is used for cannot be
  chosen; it brings everything its provider offers.

## Licence

GPL-3.0-only. The cosmic-pim crates it links are MPL-2.0; see
[LICENSING.md](LICENSING.md).
