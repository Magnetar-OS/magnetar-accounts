# Changelog

All notable changes to Accounts are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
- An address already in the list, for the same provider, is not added a
  second time; the page says which account it already is.
