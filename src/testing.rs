// SPDX-License-Identifier: GPL-3.0-only

//! A private session bus with a stand-in GNOME Online Accounts daemon on it,
//! so the code that talks to the daemon is exercised over a real bus without
//! touching the user's session or their accounts.

use std::io::BufRead as _;

use zbus::blocking::Connection;
use zbus::zvariant::OwnedObjectPath;

const BUS: &str = "org.gnome.OnlineAccounts";
const ROOT: &str = "/org/gnome/OnlineAccounts";

/// A `dbus-daemon` of our own, torn down on drop.
pub struct PrivateBus {
    child: std::process::Child,
    address: String,
    _dir: tempfile::TempDir,
}

impl PrivateBus {
    /// Starts the bus. With `installed`, the bus can start Online Accounts —
    /// it lists the name as activatable — as a desktop with the package does;
    /// actually starting it fails, which is never asked for.
    ///
    /// # Panics
    ///
    /// When `dbus-daemon` is not installed or does not start: a test that
    /// needs a bus must fail loudly rather than pass without one.
    pub fn start(installed: bool) -> Self {
        let dir = tempfile::tempdir().expect("a temporary directory for the bus");
        let services = dir.path().join("services");
        std::fs::create_dir(&services).expect("the service directory");
        if installed {
            std::fs::write(
                services.join(format!("{BUS}.service")),
                format!("[D-BUS Service]\nName={BUS}\nExec=/bin/false\n"),
            )
            .expect("the service file");
        }
        let config = dir.path().join("bus.conf");
        std::fs::write(
            &config,
            // `/tmp` rather than the temporary directory: a socket path has a
            // 108-byte limit, and `TMPDIR` can be longer than that.
            format!(
                r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <auth>EXTERNAL</auth>
  <servicedir>{}</servicedir>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#,
                services.display()
            ),
        )
        .expect("writing the bus configuration");

        let mut child = std::process::Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .arg("--nofork")
            .arg("--print-address=1")
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("dbus-daemon must be installed to run the Online Accounts tests");

        let stdout = child.stdout.take().expect("the bus's stdout");
        let mut address = String::new();
        std::io::BufReader::new(stdout)
            .read_line(&mut address)
            .expect("the bus prints its address");

        Self {
            child,
            address: address.trim().to_owned(),
            _dir: dir,
        }
    }

    /// A new connection to this bus.
    pub fn connect(&self) -> Connection {
        zbus::blocking::connection::Builder::address(self.address.as_str())
            .expect("a valid bus address")
            .build()
            .expect("connecting to the private bus")
    }

    /// Puts a stand-in daemon on the bus holding `accounts`. Dropping it takes
    /// the daemon off the bus.
    pub fn serve(&self, accounts: &[FakeAccount]) -> FakeDaemon {
        let connection = zbus::blocking::connection::Builder::address(self.address.as_str())
            .expect("a valid bus address")
            .serve_at(ROOT, zbus::fdo::ObjectManager)
            .expect("the object manager")
            .build()
            .expect("the stand-in daemon");
        let daemon = FakeDaemon { connection };
        for account in accounts {
            daemon.add(account);
        }
        daemon
            .connection
            .request_name(BUS)
            .expect("the Online Accounts name");
        daemon
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The stand-in daemon. Accounts added or removed while it runs announce
/// themselves through the object manager, as GOA's do.
pub struct FakeDaemon {
    connection: Connection,
}

impl FakeDaemon {
    fn path(id: &str) -> OwnedObjectPath {
        OwnedObjectPath::try_from(format!("{ROOT}/Accounts/{id}")).expect("a valid object path")
    }

    /// Adds an account: its OAuth interface first, then the account itself,
    /// so it is complete once it is visible as an account.
    pub fn add(&self, account: &FakeAccount) {
        let server = self.connection.object_server();
        let path = Self::path(&account.id);
        server
            .at(
                &path,
                FakeOAuth2 {
                    token: format!("token-for-{}", account.id),
                },
            )
            .expect("the OAuth2 interface");
        server
            .at(&path, account.clone())
            .expect("the account interface");
    }

    /// Removes the account with GOA's id `id`.
    pub fn remove(&self, id: &str) {
        let server = self.connection.object_server();
        let path = Self::path(id);
        server
            .remove::<FakeAccount, _>(&path)
            .expect("the account was there");
        server
            .remove::<FakeOAuth2, _>(&path)
            .expect("its OAuth2 interface was there");
    }
}

/// One account as the stand-in daemon presents it.
#[derive(Clone)]
pub struct FakeAccount {
    id: String,
    provider_type: String,
    identity: String,
}

impl FakeAccount {
    pub fn google(id: &str, identity: &str) -> Self {
        Self {
            id: id.to_owned(),
            provider_type: "google".to_owned(),
            identity: identity.to_owned(),
        }
    }
}

#[zbus::interface(name = "org.gnome.OnlineAccounts.Account")]
impl FakeAccount {
    #[zbus(property)]
    fn id(&self) -> String {
        self.id.clone()
    }

    #[zbus(property)]
    fn provider_type(&self) -> String {
        self.provider_type.clone()
    }

    #[zbus(property)]
    fn presentation_identity(&self) -> String {
        self.identity.clone()
    }

    #[zbus(property)]
    fn identity(&self) -> String {
        "1234567890".to_owned()
    }

    #[zbus(property)]
    fn attention_needed(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn mail_disabled(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn calendar_disabled(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn contacts_disabled(&self) -> bool {
        false
    }

    fn ensure_credentials(&self) -> i32 {
        3000
    }
}

struct FakeOAuth2 {
    token: String,
}

#[zbus::interface(name = "org.gnome.OnlineAccounts.OAuth2Based")]
impl FakeOAuth2 {
    fn get_access_token(&self) -> (String, i32) {
        (self.token.clone(), 3000)
    }
}
