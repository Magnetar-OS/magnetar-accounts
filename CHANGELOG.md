# Changelog

All notable changes to Accounts are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.5] - 2026-11-07

### Changed

- Rebuilt against the current COSMIC libraries (libcosmic `dd108aa`).
- Built on zbus 6 and cosmic-pim 5, as libcosmic now is. The background Online
  Accounts watch works as before.

## [0.2.4] - 2026-11-01

### Changed

- Built on cosmic-ext-widgets 1.3.1 and the latest releases of its
  dependencies: tokio 1.54, open 5.4.5 and rust-embed 8.13.

## [0.2.3] - 2026-11-01

### Fixed

- The About button in the header bar is drawn whole at Spacious interface
  density; the header used to cut off its top half. It has a tooltip and an
  accessible name now, and dims with the window controls when the window is
  not focused.

## [0.2.2] - 2026-11-01

### Changed

- Built on cosmic-pim 4 (mail-parser 1.0, rusqlite 0.41).

## [0.2.1] - 2026-11-01

### Changed

- Rebuilt against the current COSMIC libraries (libcosmic `95236a9`).

## [0.2.0] - 2026-10-21

### Changed

- Rebuilt against the current COSMIC libraries (libcosmic `430f3d4`) with
  Rust 1.99.1.
- Escape pressed in a text field now does what Escape does elsewhere in the
  window — leaves the add form for the list. It used to only take the
  focus out of the field, so it took a second press.

## [0.1.0] - 2026-10-13

### Added

- The window: the accounts there are, what each brings to Envelope, Slate and
  Circle, and how it signs in; adding one by address; removing one after a
  confirmation.
- Adding by password, with the servers found from the address and the
  password tried before it is kept.
- The provider's own browser sign-in, naming the account after whoever
  signed in.
- GNOME Online Accounts: its Google and Microsoft accounts are linked in, and
  unlinked when removed there. Its window opens from here.
- `--add` and `--for=<mail|calendar|contacts>`, for an application's
  first-run page; a second launch raises the window already open.
- `list` and `adopt` on the command line.
- A welcome at a new user's first login, from an autostart entry: the add
  page, once, and only when there is no account yet.
- Accounts added or removed in GNOME Online Accounts follow into the list in
  the background, from an autostart entry (`watch`), without this window being
  opened. Without Online Accounts installed it does nothing.
- An address already in the list, for the same provider, is not added a
  second time; the page says which account it already is.
- Greek translation.

[Unreleased]: https://github.com/Magnetar-OS/magnetar-accounts/compare/v0.2.5...HEAD
[0.2.5]: https://github.com/Magnetar-OS/magnetar-accounts/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/Magnetar-OS/magnetar-accounts/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/Magnetar-OS/magnetar-accounts/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/Magnetar-OS/magnetar-accounts/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/Magnetar-OS/magnetar-accounts/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/Magnetar-OS/magnetar-accounts/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Magnetar-OS/magnetar-accounts/releases/tag/v0.1.0
