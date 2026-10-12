app-title = Accounts
# Also the desktop entry's Comment and the AppStream summary.
app-comment = Sign in once, and mail, calendars and contacts follow
repository = Repository

## The list

accounts = Accounts
accounts-intro = Add an account once, and Envelope, Slate and Circle all use it: mail in Envelope, calendars in Slate, contacts in Circle.
no-accounts = No accounts yet.
accounts-unreadable = The account list could not be read: { $reason }
add-account = Add account…
remove = Remove
cancel = Cancel
# On an account whose sign-in GNOME Online Accounts keeps; it opens that window.
manage = Manage…
remove-title = Remove { $name }?
remove-body = Envelope, Slate and Circle stop using it, and its password or sign-in is forgotten. Mail, events and contacts already on this computer are kept.
remove-failed = The account could not be removed: { $reason }
added = { $name } was added. Envelope, Slate and Circle use it from now on.
linked = { $count ->
        [one] An account from Online Accounts was added.
       *[other] { $count } accounts from Online Accounts were added.
    }
online-accounts-note = Google and Microsoft accounts you add in GNOME Online Accounts appear here as well.
open-online-accounts = Open Online Accounts
online-accounts-failed = Accounts from Online Accounts could not be brought in: { $reason }
online-accounts-missing = Online Accounts is running, but its window ({ $program }) is not installed.

# How an account signs in, after what it brings.
via-password = password
via-sign-in = signed in
via-online-accounts = through Online Accounts

mail = Mail
calendar = Calendar
contacts = Contacts
nothing-to-sync = Nothing to sync

## Adding one

back-to-accounts = Accounts
add-account-title = Add an account
purpose-mail = Envelope needs an account with mail. Add one here, and its calendars and contacts come too, where the provider has them.
purpose-calendar = Slate needs an account with calendars. Add one here, and its mail and contacts come too, where the provider has them.
purpose-contacts = Circle needs an account with contacts. Add one here, and its mail and calendars come too, where the provider has them.
address = Email address
other-provider = Your provider
# The line under the address: which provider, and what adding it brings.
brings = { $provider }: { $services }
other-ways = Other ways to sign in
continue-in-browser = Continue in the browser with { $provider }
use-online-accounts = Sign in with Online Accounts
online-accounts-explained = GNOME Online Accounts signs in for you and keeps the account signed in. Its window opens; add the account there, then close it.
name = Name
name-hint = On the mail you send, and in the account list. Optional.
password = Password
where-to-get-one = Where to get one
add = Add
checking = Finding the servers and checking the password…
signing-in = Finish signing in in your browser. This page carries on by itself.
waiting-for-online-accounts = Add the account in the Online Accounts window, then close it.
no-route = { $provider } can only be added with a browser sign-in, and this computer has none set up for it yet.
no-route-online-accounts-hint = Installing GNOME Online Accounts gives it one.
rejected = The password was not accepted.
rejected-with-hint = The password was not accepted. { $hint }
unreachable = The server could not be reached: { $reason }
no-server = No mail server could be found for this address. Check the address, or add the account in Envelope, where the servers can be typed in.
