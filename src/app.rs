// SPDX-License-Identifier: GPL-3.0-only

//! The window: the accounts there are, and adding one.
//!
//! Two pages. The list says what each account is and what it brings to the
//! applications; the add page takes an address and offers the ways in that
//! can actually work on this computer, best first. Every call that touches
//! the network or the keychain runs off the UI thread and comes back as a
//! message.

use std::process::Child;
use std::time::Duration;

use cosmic::app::{Core, Task, context_drawer};
use cosmic::iced::{Alignment, Length, Subscription};
use cosmic::prelude::*;
use cosmic::widget::{self, about::About};
use cosmic_pim_accounts::{Account, AccountStore, Registry};
use cosmic_pim_auth::OnlineAccounts;
use cosmic_pim_sync::setup::{self, Plan, Route, Services, SetupError};

use crate::{Purpose, Request, fl};

const APP_ID: &str = "com.magnetaros.Accounts";
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const APP_ICON: &[u8] =
    include_bytes!("../resources/icons/hicolor/scalable/apps/com.magnetaros.Accounts.svg");

/// GNOME's own window for Online Accounts, as packaged on its own for
/// desktops other than GNOME.
const ONLINE_ACCOUNTS_WINDOW: &str = "gnome-online-accounts-gtk";

/// How often to look whether the Online Accounts window has closed, while it
/// is open.
const ONLINE_WINDOW_POLL: Duration = Duration::from_millis(1500);

/// The widest the page's content grows: a form stretched across a wide
/// window is harder to read, not easier.
const CONTENT_WIDTH: f32 = 640.0;

/// What a launch — or a second launch — asks the window for.
#[derive(Clone, Debug, Default)]
pub struct Flags {
    handover: Option<Handover>,
    request: Request,
}

impl Flags {
    #[must_use]
    pub fn new(request: Request) -> Self {
        let handover = (request != Request::default()).then(|| Handover(request.clone()));
        Self { handover, request }
    }
}

/// A [`Request`] on its way across the bus to the window already open.
#[derive(Clone, Debug)]
pub struct Handover(Request);

impl std::fmt::Display for Handover {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&serde_json::to_string(&self.0).unwrap_or_else(|_| "{}".to_owned()))
    }
}

impl cosmic::app::CosmicFlags for Flags {
    type SubCommand = Handover;
    type Args = Vec<String>;

    /// `None` for a bare launch, which only raises the window.
    fn action(&self) -> Option<&Self::SubCommand> {
        self.handover.as_ref()
    }
}

/// Which page is showing. The form is boxed: the list page is most of the
/// window's life, and carries nothing.
#[derive(Debug)]
enum Page {
    List,
    Add(Box<AddForm>),
}

/// The add page's state.
#[derive(Debug, Default)]
struct AddForm {
    /// Which application sent the person here, and for what.
    purpose: Option<Purpose>,
    address: String,
    name: String,
    password: String,
    /// What the address means, recomputed as it is typed.
    plan: Option<Plan>,
    busy: Option<Busy>,
    error: Option<String>,
}

/// What the add page is waiting for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Busy {
    /// Finding the servers and trying the password.
    Checking,
    /// The browser is open on the provider's sign-in.
    SigningIn,
    /// GNOME's Online Accounts window is open.
    OnlineAccounts,
}

#[derive(Clone, Debug)]
pub enum Message {
    /// The account list, read off the UI thread.
    Loaded(Result<Vec<Account>, String>),
    /// Whether GNOME Online Accounts is running.
    OnlineChecked(Option<OnlineAccounts>),
    /// Online Accounts' accounts linked in: the list after, and how many were
    /// new.
    Adopted(Result<(Vec<Account>, usize), String>),
    ShowAdd(Option<Purpose>),
    ShowList,
    Address(String),
    Name(String),
    Password(String),
    AddWithPassword,
    SignIn,
    UseOnlineAccounts,
    /// An account was added: the list after, and what it is called.
    Added(Result<(Vec<Account>, String), AddFailure>),
    /// Time to look whether the Online Accounts window has closed.
    PollOnlineWindow,
    RemoveRequested(String),
    RemoveConfirmed,
    RemoveCancelled,
    Removed(Result<Vec<Account>, String>),
    Open(String),
    ToggleAbout,
    /// A second launch asking for a page.
    Activated(Request),
}

/// Why an account was not added, in the terms the page speaks.
#[derive(Clone, Debug)]
pub enum AddFailure {
    /// The server refused the password.
    Rejected,
    Unreachable(String),
    NoServer,
    /// The list already has an account for this address and provider: the
    /// one it is listed as.
    Exists(String),
    Other(String),
}

impl From<SetupError> for AddFailure {
    fn from(why: SetupError) -> Self {
        match why {
            SetupError::Rejected(_) => Self::Rejected,
            SetupError::Unreachable(reason) => Self::Unreachable(reason),
            SetupError::NoServer(_) => Self::NoServer,
            other => Self::Other(other.to_string()),
        }
    }
}

pub struct AppModel {
    core: Core,
    about: About,
    registry: Registry,
    accounts: Vec<Account>,
    /// Why the list could not be read, when it could not.
    load_error: Option<String>,
    /// GNOME Online Accounts, when it is running.
    online: Option<OnlineAccounts>,
    /// Its window, while it is open.
    online_window: Option<Child>,
    page: Page,
    /// The id of the account a removal is being confirmed for.
    removing: Option<String>,
    /// The line at the top of the list saying what just happened.
    notice: Option<String>,
}

/// Reads the account list. Blocking: it may ask the keychain.
fn read_accounts() -> Result<Vec<Account>, String> {
    AccountStore::open_default()
        .map(|store| store.accounts().to_vec())
        .map_err(|why| why.to_string())
}

/// Runs `work` on a blocking thread and turns its answer into a message.
fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(T) -> Message + Send + 'static,
    lost: impl FnOnce(String) -> Message + Send + 'static,
) -> Task<Message> {
    cosmic::task::future(async move {
        match tokio::task::spawn_blocking(work).await {
            Ok(value) => done(value),
            Err(why) => lost(why.to_string()),
        }
    })
}

impl AppModel {
    /// Opens the page a request asks for.
    fn open(&mut self, request: &Request) {
        if request.add {
            self.show_add(request.purpose);
        }
    }

    fn show_add(&mut self, purpose: Option<Purpose>) {
        // Keep what was typed when the page is already open; a second
        // application asking only changes why.
        if let Page::Add(form) = &mut self.page {
            form.purpose = purpose.or(form.purpose);
            return;
        }
        self.page = Page::Add(Box::new(AddForm {
            purpose,
            ..AddForm::default()
        }));
        self.notice = None;
    }

    fn form(&mut self) -> Option<&mut AddForm> {
        match &mut self.page {
            Page::Add(form) => Some(form),
            Page::List => None,
        }
    }

    /// Links whatever Online Accounts holds, off the UI thread.
    fn adopt(&self) -> Task<Message> {
        let Some(service) = self.online.clone() else {
            return Task::none();
        };
        let registry = self.registry.clone();
        blocking(
            move || {
                let mut store = AccountStore::open_default().map_err(|why| why.to_string())?;
                let adopted = setup::adopt_online_accounts(&mut store, &registry, &service)
                    .map_err(|why| why.to_string())?;
                for (id, why) in &adopted.failed {
                    tracing::warn!(id, why, "an Online Accounts account could not be linked");
                }
                Ok((store.accounts().to_vec(), adopted.linked.len()))
            },
            Message::Adopted,
            |why| Message::Adopted(Err(why)),
        )
    }

    /// Opens GNOME's Online Accounts window, where Google and Microsoft sign
    /// in under GNOME's registration.
    fn open_online_accounts(&mut self) -> Task<Message> {
        if self.online_window.is_some() {
            return Task::none();
        }
        match std::process::Command::new(ONLINE_ACCOUNTS_WINDOW).spawn() {
            Ok(child) => {
                self.online_window = Some(child);
                if let Some(form) = self.form() {
                    form.busy = Some(Busy::OnlineAccounts);
                    form.error = None;
                }
            }
            Err(why) => {
                tracing::warn!(%why, "cannot open {ONLINE_ACCOUNTS_WINDOW}");
                let message = fl!("online-accounts-missing", program = ONLINE_ACCOUNTS_WINDOW);
                match self.form() {
                    Some(form) => form.error = Some(message),
                    None => self.notice = Some(message),
                }
            }
        }
        Task::none()
    }

    fn add_with_password(&mut self) -> Task<Message> {
        let registry = self.registry.clone();
        let Some(form) = self.form() else {
            return Task::none();
        };
        let Some(plan) = form.plan.clone() else {
            return Task::none();
        };
        if form.password.is_empty() || form.busy.is_some() {
            return Task::none();
        }
        form.busy = Some(Busy::Checking);
        form.error = None;
        let (name, password) = (form.name.clone(), form.password.clone());
        blocking(
            move || {
                let mut store = AccountStore::open_default()
                    .map_err(|why| AddFailure::Other(why.to_string()))?;
                refuse_a_second(&store, &plan)?;
                let id = setup::add_with_password(&mut store, &registry, &plan, &name, &password)?;
                Ok(added(&store, &id))
            },
            Message::Added,
            |why| Message::Added(Err(AddFailure::Other(why))),
        )
    }

    fn sign_in(&mut self) -> Task<Message> {
        let Some(form) = self.form() else {
            return Task::none();
        };
        let Some(plan) = form.plan.clone() else {
            return Task::none();
        };
        if form.busy.is_some() {
            return Task::none();
        }
        form.busy = Some(Busy::SigningIn);
        form.error = None;
        blocking(
            move || {
                let mut store = AccountStore::open_default()
                    .map_err(|why| AddFailure::Other(why.to_string()))?;
                refuse_a_second(&store, &plan)?;
                // The user's own browser, never a view of ours — see
                // `cosmic_pim_auth::flow`.
                let id = setup::sign_in(&mut store, &plan, |url| open::that_detached(url))?;
                Ok(added(&store, &id))
            },
            Message::Added,
            |why| Message::Added(Err(AddFailure::Other(why))),
        )
    }

    fn remove_confirmed(&mut self) -> Task<Message> {
        let Some(id) = self.removing.take() else {
            return Task::none();
        };
        blocking(
            move || {
                let mut store = AccountStore::open_default().map_err(|why| why.to_string())?;
                store.remove(&id).map_err(|why| why.to_string())?;
                Ok(store.accounts().to_vec())
            },
            Message::Removed,
            |why| Message::Removed(Err(why)),
        )
    }

    /// What the page says an account brings, and how it signs in.
    fn describe(&self, account: &Account) -> String {
        let services = services_line(setup::services_of(account, &self.registry));
        let how = if account.online_account().is_some() {
            fl!("via-online-accounts")
        } else if account.is_oauth() {
            fl!("via-sign-in")
        } else {
            fl!("via-password")
        };
        format!("{} · {services} · {how}", account.username)
    }

    fn list_page(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let mut column = widget::column::with_capacity(8)
            .spacing(spacing.space_m)
            .push(widget::text::title3(fl!("accounts")))
            .push(wrapped(widget::text::body(fl!("accounts-intro"))));

        if let Some(notice) = &self.notice {
            column = column.push(wrapped(widget::text::body(notice.clone())));
        }
        if let Some(why) = &self.load_error {
            column = column.push(error_text(fl!("accounts-unreadable", reason = why.clone())));
        }

        if self.accounts.is_empty() {
            column = column.push(wrapped(widget::text::body(fl!("no-accounts"))));
        } else {
            let mut section = widget::settings::section();
            for account in &self.accounts {
                // An account Online Accounts holds is removed there: removed
                // here, the next look at Online Accounts would bring it back.
                let control = if account.online_account().is_some() {
                    widget::button::text(fl!("manage")).on_press(Message::UseOnlineAccounts)
                } else {
                    widget::button::text(fl!("remove"))
                        .on_press(Message::RemoveRequested(account.id.clone()))
                };
                section = section.add(
                    widget::settings::item::builder(account.display_name.clone())
                        .description(self.describe(account))
                        .control(control),
                );
            }
            column = column.push(section);
        }

        column = column
            .push(widget::button::suggested(fl!("add-account")).on_press(Message::ShowAdd(None)));

        if self.online.is_some() {
            column = column.push(
                widget::column::with_capacity(2)
                    .spacing(spacing.space_xxs)
                    .push(wrapped(widget::text::caption(fl!("online-accounts-note"))))
                    .push(
                        widget::button::standard(fl!("open-online-accounts"))
                            .on_press(Message::UseOnlineAccounts),
                    ),
            );
        }

        column.into()
    }

    fn add_page<'a>(&'a self, form: &'a AddForm) -> Element<'a, Message> {
        let spacing = cosmic::theme::spacing();
        let mut column = widget::column::with_capacity(10)
            .spacing(spacing.space_m)
            .push(
                widget::button::text(fl!("back-to-accounts"))
                    .leading_icon(widget::icon::from_name("go-previous-symbolic"))
                    .on_press(Message::ShowList),
            )
            .push(widget::text::title3(fl!("add-account-title")));

        if let Some(purpose) = form.purpose {
            column = column.push(wrapped(widget::text::body(match purpose {
                Purpose::Mail => fl!("purpose-mail"),
                Purpose::Calendar => fl!("purpose-calendar"),
                Purpose::Contacts => fl!("purpose-contacts"),
            })));
        }

        // Without `on_input` a field is shown disabled: the address cannot
        // change under a sign-in that is already using it.
        let mut address = widget::text_input("you@example.com", &form.address).width(Length::Fill);
        if form.busy.is_none() {
            address = address.on_input(Message::Address);
        }
        column = column.push(
            widget::settings::section()
                .add(widget::settings::item::builder(fl!("address")).flex_control(address)),
        );

        if let Some(plan) = &form.plan {
            column = column.push(self.routes(form, plan));
        }

        if let Some(waiting) = form.busy {
            column = column.push(wrapped(widget::text::body(match waiting {
                Busy::Checking => fl!("checking"),
                Busy::SigningIn => fl!("signing-in"),
                Busy::OnlineAccounts => fl!("waiting-for-online-accounts"),
            })));
        }
        if let Some(error) = &form.error {
            column = column.push(error_text(error.clone()));
        }

        column.into()
    }

    /// The ways in, best first; the first is the one the page leads with.
    fn routes<'a>(&'a self, form: &'a AddForm, plan: &'a Plan) -> Element<'a, Message> {
        let spacing = cosmic::theme::spacing();
        let provider = plan
            .provider
            .as_ref()
            .map_or_else(|| fl!("other-provider"), |provider| provider.name.clone());

        let mut column = widget::column::with_capacity(4).spacing(spacing.space_s);

        let Some(first) = plan.routes.first() else {
            let hint = if plan
                .provider
                .as_ref()
                .is_some_and(|p| cosmic_pim_auth::online_accounts::serves(&p.id))
            {
                fl!("no-route-online-accounts-hint")
            } else {
                String::new()
            };
            return column
                .push(error_text(fl!("no-route", provider = provider)))
                .push(wrapped(widget::text::caption(hint)))
                .into();
        };

        column = column.push(wrapped(widget::text::body(fl!(
            "brings",
            provider = provider,
            services = services_line(plan.services(first))
        ))));
        column = column.push(self.route(form, plan, first, true));

        if plan.routes.len() > 1 {
            column = column.push(widget::text::heading(fl!("other-ways")));
            for route in &plan.routes[1..] {
                column = column
                    .push(wrapped(widget::text::caption(services_line(
                        plan.services(route),
                    ))))
                    .push(self.route(form, plan, route, false));
            }
        }
        column.into()
    }

    /// One way in: a button, or for a password the fields it needs.
    fn route<'a>(
        &'a self,
        form: &'a AddForm,
        plan: &'a Plan,
        route: &'a Route,
        lead: bool,
    ) -> Element<'a, Message> {
        let spacing = cosmic::theme::spacing();
        let idle = form.busy.is_none();
        let button = |label: String, message: Message| {
            let button = if lead {
                widget::button::suggested(label)
            } else {
                widget::button::standard(label)
            };
            button.on_press_maybe(idle.then_some(message))
        };

        match route {
            Route::SignIn => {
                let provider = plan.provider.as_ref().map_or("", |p| p.name.as_str());
                button(
                    fl!("continue-in-browser", provider = provider.to_owned()),
                    Message::SignIn,
                )
                .into()
            }
            Route::OnlineAccounts => widget::column::with_capacity(2)
                .spacing(spacing.space_xxs)
                .push(wrapped(widget::text::caption(fl!(
                    "online-accounts-explained"
                ))))
                .push(button(
                    fl!("use-online-accounts"),
                    Message::UseOnlineAccounts,
                ))
                .into(),
            Route::Password { hint, help_url } => {
                let mut name =
                    widget::text_input(plan.address.clone(), &form.name).width(Length::Fill);
                let mut password =
                    widget::secure_input(fl!("password"), &form.password, None, true)
                        .width(Length::Fill);
                if idle {
                    name = name.on_input(Message::Name);
                    password = password
                        .on_input(Message::Password)
                        .on_submit(|_| Message::AddWithPassword);
                }
                let mut fields = widget::settings::section()
                    .add(
                        widget::settings::item::builder(fl!("name"))
                            .description(fl!("name-hint"))
                            .flex_control(name),
                    )
                    .add(widget::settings::item::builder(fl!("password")).flex_control(password));
                if let Some(hint) = hint {
                    fields = fields.add(wrapped(widget::text::caption(hint.clone())));
                }

                let mut row = widget::row::with_capacity(2)
                    .spacing(spacing.space_s)
                    .align_y(Alignment::Center);
                if let Some(url) = help_url {
                    row = row.push(
                        widget::button::link(fl!("where-to-get-one"))
                            .on_press(Message::Open(url.clone())),
                    );
                }
                row = row.push(widget::space::horizontal()).push(
                    button(fl!("add"), Message::AddWithPassword).on_press_maybe(
                        (idle && !form.password.is_empty()).then_some(Message::AddWithPassword),
                    ),
                );

                widget::column::with_capacity(2)
                    .spacing(spacing.space_s)
                    .push(fields)
                    .push(row)
                    .into()
            }
            // `Route` is non-exhaustive: a way in added by a later cosmic-pim
            // is one this build has no means to drive, so it offers nothing.
            _ => widget::column::with_capacity(0).into(),
        }
    }

    /// Whether the Online Accounts window has closed, and if so, what it
    /// changed.
    fn poll_online_window(&mut self) -> Task<Message> {
        let closed = match self.online_window.as_mut().map(Child::try_wait) {
            Some(Ok(Some(_)) | Err(_)) => true,
            Some(Ok(None)) | None => false,
        };
        if !closed {
            return Task::none();
        }
        self.online_window = None;
        if let Some(form) = self.form()
            && form.busy == Some(Busy::OnlineAccounts)
        {
            form.busy = None;
        }
        self.adopt()
    }
}

/// The account the list already holds for the address and provider `plan`
/// is about, if there is one.
///
/// `setup::add_with_password` and `setup::sign_in` store what they are given
/// without looking: a second Add for the same address would make a second
/// account, and every application would sync the same mailbox and calendars
/// twice.
fn already_added<'a>(accounts: &'a [Account], plan: &Plan) -> Option<&'a Account> {
    let address = plan.address.trim();
    let provider = plan.provider.as_ref().map(|provider| provider.id.as_str());
    accounts.iter().find(|account| {
        account.username.trim().eq_ignore_ascii_case(address)
            && account.provider.as_deref() == provider
    })
}

/// [`already_added`] as the reason not to go on.
fn refuse_a_second(store: &AccountStore, plan: &Plan) -> Result<(), AddFailure> {
    match already_added(store.accounts(), plan) {
        Some(account) => Err(AddFailure::Exists(account.display_name.clone())),
        None => Ok(()),
    }
}

/// What an account that has just been added looks like to the list.
fn added(store: &AccountStore, id: &str) -> (Vec<Account>, String) {
    let name = store
        .get(id)
        .map_or_else(|| id.to_owned(), |account| account.display_name.clone());
    (store.accounts().to_vec(), name)
}

/// "Mail, Calendar, Contacts", or what of it there is.
fn services_line(services: Services) -> String {
    let names: Vec<String> = [
        (services.mail, fl!("mail")),
        (services.calendar, fl!("calendar")),
        (services.contacts, fl!("contacts")),
    ]
    .into_iter()
    .filter_map(|(on, name)| on.then_some(name))
    .collect();
    if names.is_empty() {
        fl!("nothing-to-sync")
    } else {
        names.join(", ")
    }
}

fn wrapped<'a>(
    text: cosmic::widget::Text<'a, cosmic::Theme, cosmic::Renderer>,
) -> Element<'a, Message> {
    text.wrapping(cosmic::iced::core::text::Wrapping::Word)
        .into()
}

/// Text in the theme's destructive colour, for what went wrong — as
/// Envelope draws its own.
fn error_text<'a>(text: String) -> Element<'a, Message> {
    widget::text::body(text)
        .class(cosmic::theme::Text::Custom(|theme| {
            cosmic::iced::widget::text::Style {
                color: Some(theme.cosmic().destructive_color().into()),
                ..Default::default()
            }
        }))
        .wrapping(cosmic::iced::core::text::Wrapping::Word)
        .into()
}

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = Flags;
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let about = About::default()
            .name(fl!("app-title"))
            .icon(widget::icon::from_svg_bytes(APP_ICON))
            .version(env!("CARGO_PKG_VERSION"))
            .license(env!("CARGO_PKG_LICENSE"))
            .links([(fl!("repository"), REPOSITORY)]);

        let mut app = Self {
            core,
            about,
            registry: Registry::load(),
            accounts: Vec::new(),
            load_error: None,
            online: None,
            online_window: None,
            page: Page::List,
            removing: None,
            notice: None,
        };
        app.open(&flags.request);

        let load = blocking(read_accounts, Message::Loaded, |why| {
            Message::Loaded(Err(why))
        });
        // Asked once, here, rather than while the address is typed: the
        // answer does not change while the window is open, and a bus round
        // trip per keystroke would.
        let online = blocking(
            || match OnlineAccounts::connect() {
                Ok(online) => online,
                Err(why) => {
                    tracing::info!(%why, "Online Accounts is not reachable");
                    None
                }
            },
            Message::OnlineChecked,
            |_| Message::OnlineChecked(None),
        );
        (app, Task::batch([load, online]))
    }

    fn dbus_activation(&mut self, msg: cosmic::dbus_activation::Message) -> Task<Self::Message> {
        use cosmic::dbus_activation::Details;

        match msg.msg {
            Details::ActivateAction { action, .. } => match serde_json::from_str(&action) {
                Ok(request) => {
                    cosmic::task::message(cosmic::Action::App(Message::Activated(request)))
                }
                Err(why) => {
                    tracing::warn!(%why, action, "unknown activation action");
                    Task::none()
                }
            },
            // A plain launch raises the window, which the runtime has done.
            Details::Activate | Details::Open { .. } => Task::none(),
        }
    }

    fn on_escape(&mut self) -> Task<Self::Message> {
        if self.removing.take().is_some() {
            return Task::none();
        }
        if let Page::Add(form) = &self.page
            && form.busy.is_none()
        {
            self.page = Page::List;
        }
        Task::none()
    }

    fn context_drawer(&self) -> Option<context_drawer::ContextDrawer<'_, Self::Message>> {
        if !self.core.window.show_context {
            return None;
        }
        Some(context_drawer::about(
            &self.about,
            |url| Message::Open(url.to_string()),
            Message::ToggleAbout,
        ))
    }

    fn header_end(&self) -> Vec<Element<'_, Self::Message>> {
        vec![
            widget::button::icon(widget::icon::from_name("help-about-symbolic"))
                .on_press(Message::ToggleAbout)
                .into(),
        ]
    }

    fn dialog(&self) -> Option<Element<'_, Self::Message>> {
        let id = self.removing.as_ref()?;
        let name = self
            .accounts
            .iter()
            .find(|account| &account.id == id)
            .map_or_else(|| id.clone(), |account| account.display_name.clone());
        Some(
            widget::dialog()
                .title(fl!("remove-title", name = name))
                .body(fl!("remove-body"))
                .primary_action(
                    widget::button::destructive(fl!("remove")).on_press(Message::RemoveConfirmed),
                )
                .secondary_action(
                    widget::button::text(fl!("cancel")).on_press(Message::RemoveCancelled),
                )
                .into(),
        )
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        if self.online_window.is_some() {
            cosmic::iced::time::every(ONLINE_WINDOW_POLL).map(|_| Message::PollOnlineWindow)
        } else {
            Subscription::none()
        }
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let spacing = cosmic::theme::spacing();
        let page = match &self.page {
            Page::List => self.list_page(),
            Page::Add(form) => self.add_page(form),
        };
        widget::scrollable(
            widget::container(widget::container(page).max_width(CONTENT_WIDTH))
                .center_x(Length::Fill)
                .padding(spacing.space_l),
        )
        .height(Length::Fill)
        .into()
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::Loaded(Ok(accounts)) => {
                self.accounts = accounts;
                self.load_error = None;
            }
            Message::Loaded(Err(why)) => self.load_error = Some(why),
            Message::OnlineChecked(online) => {
                self.online = online;
                // Recompute: an address typed before the answer arrived may
                // have a way in now.
                if let Page::Add(form) = &mut self.page {
                    form.plan = setup::plan(&self.registry, &form.address, self.online.is_some());
                }
                return self.adopt();
            }
            Message::Adopted(Ok((accounts, linked))) => {
                self.accounts = accounts;
                if linked > 0 {
                    self.notice = Some(fl!("linked", count = linked));
                    self.page = Page::List;
                }
            }
            Message::Adopted(Err(why)) => {
                tracing::warn!(why, "Online Accounts could not be brought in");
                self.notice = Some(fl!("online-accounts-failed", reason = why));
            }
            Message::ShowAdd(purpose) => self.show_add(purpose),
            Message::ShowList => {
                if !matches!(&self.page, Page::Add(form) if form.busy.is_some()) {
                    self.page = Page::List;
                }
            }
            Message::Address(address) => {
                let online = self.online.is_some();
                let registry = self.registry.clone();
                if let Some(form) = self.form() {
                    form.plan = setup::plan(&registry, &address, online);
                    form.address = address;
                    form.error = None;
                }
            }
            Message::Name(name) => {
                if let Some(form) = self.form() {
                    form.name = name;
                }
            }
            Message::Password(password) => {
                if let Some(form) = self.form() {
                    form.password = password;
                    form.error = None;
                }
            }
            Message::AddWithPassword => return self.add_with_password(),
            Message::SignIn => return self.sign_in(),
            Message::UseOnlineAccounts => return self.open_online_accounts(),
            Message::Added(Ok((accounts, name))) => {
                self.accounts = accounts;
                self.notice = Some(fl!("added", name = name));
                self.page = Page::List;
            }
            Message::Added(Err(failure)) => {
                let hint = match &self.page {
                    Page::Add(form) => form.plan.as_ref().and_then(|plan| {
                        plan.routes.iter().find_map(|route| match route {
                            Route::Password { hint, .. } => hint.clone(),
                            _ => None,
                        })
                    }),
                    Page::List => None,
                };
                if let Some(form) = self.form() {
                    form.busy = None;
                    form.error = Some(match failure {
                        AddFailure::Rejected => match hint {
                            Some(hint) => fl!("rejected-with-hint", hint = hint),
                            None => fl!("rejected"),
                        },
                        AddFailure::Unreachable(reason) => fl!("unreachable", reason = reason),
                        AddFailure::NoServer => fl!("no-server"),
                        AddFailure::Exists(name) => fl!("already-added", name = name),
                        AddFailure::Other(reason) => reason,
                    });
                }
            }
            Message::PollOnlineWindow => return self.poll_online_window(),
            Message::RemoveRequested(id) => self.removing = Some(id),
            Message::RemoveConfirmed => return self.remove_confirmed(),
            Message::RemoveCancelled => self.removing = None,
            Message::Removed(Ok(accounts)) => {
                self.accounts = accounts;
                self.notice = None;
            }
            Message::Removed(Err(why)) => {
                self.notice = Some(fl!("remove-failed", reason = why));
            }
            Message::Open(url) => {
                if let Err(why) = open::that_detached(&url) {
                    tracing::warn!(%why, url, "cannot open the link");
                }
            }
            Message::ToggleAbout => {
                self.core.window.show_context = !self.core.window.show_context;
            }
            Message::Activated(request) => self.open(&request),
        }
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic::Application as _;

    fn app() -> AppModel {
        AppModel {
            core: Core::default(),
            about: About::default(),
            registry: Registry::load_from(std::path::Path::new("/nonexistent")),
            accounts: Vec::new(),
            load_error: None,
            online: None,
            online_window: None,
            page: Page::List,
            removing: None,
            notice: None,
        }
    }

    fn form(app: &AppModel) -> &AddForm {
        match &app.page {
            Page::Add(form) => form,
            Page::List => panic!("not on the add page"),
        }
    }

    #[test]
    fn an_application_asking_for_mail_opens_the_add_page_saying_so() {
        let mut app = app();

        let _ = app.update(Message::Activated(Request {
            add: true,
            purpose: Some(Purpose::Mail),
        }));

        assert_eq!(form(&app).purpose, Some(Purpose::Mail));
    }

    #[test]
    fn a_second_request_keeps_what_was_typed() {
        let mut app = app();
        let _ = app.update(Message::ShowAdd(None));
        let _ = app.update(Message::Address("ada@fastmail.com".into()));

        let _ = app.update(Message::Activated(Request {
            add: true,
            purpose: Some(Purpose::Calendar),
        }));

        assert_eq!(form(&app).address, "ada@fastmail.com");
        assert_eq!(form(&app).purpose, Some(Purpose::Calendar));
    }

    #[test]
    fn typing_an_address_works_out_its_ways_in() {
        let mut app = app();
        let _ = app.update(Message::ShowAdd(None));

        let _ = app.update(Message::Address("ada@gmail.com".into()));

        let plan = form(&app).plan.as_ref().expect("a plan for an address");
        assert_eq!(
            plan.provider.as_ref().map(|p| p.id.as_str()),
            Some("google")
        );
        assert!(matches!(plan.routes.as_slice(), [Route::Password { .. }]));
    }

    #[test]
    fn a_refused_password_says_so_with_the_providers_own_hint() {
        let mut app = app();
        let _ = app.update(Message::ShowAdd(None));
        let _ = app.update(Message::Address("ada@fastmail.com".into()));
        if let Page::Add(form) = &mut app.page {
            form.busy = Some(Busy::Checking);
        }

        let _ = app.update(Message::Added(Err(AddFailure::Rejected)));

        let form = form(&app);
        assert_eq!(form.busy, None);
        let error = form.error.as_deref().unwrap();
        assert!(error.contains("app password"), "{error}");
    }

    #[test]
    fn an_added_account_returns_to_the_list_and_says_what_was_added() {
        let mut app = app();
        let _ = app.update(Message::ShowAdd(None));
        let account = Account::new("Ada at Fastmail", "", "ada@fastmail.com");

        let _ = app.update(Message::Added(Ok((
            vec![account],
            "Ada at Fastmail".into(),
        ))));

        assert!(matches!(app.page, Page::List));
        assert_eq!(app.accounts.len(), 1);
        assert!(app.notice.as_deref().unwrap().contains("Ada at Fastmail"));
    }

    #[test]
    fn an_address_already_in_the_list_is_found_whatever_its_case() {
        let registry = Registry::load_from(std::path::Path::new("/nonexistent"));
        let fastmail = registry
            .get("fastmail")
            .unwrap()
            .account_for("ada@fastmail.com");
        let elsewhere = Account::new("Work", "", "ada@example.org");
        let accounts = [elsewhere, fastmail];

        let again = setup::plan(&registry, "Ada@Fastmail.com", false).unwrap();
        let other = setup::plan(&registry, "grace@fastmail.com", false).unwrap();

        assert_eq!(
            already_added(&accounts, &again).map(|a| a.username.as_str()),
            Some("ada@fastmail.com")
        );
        assert!(already_added(&accounts, &other).is_none());
    }

    #[test]
    fn adding_an_address_twice_says_where_it_already_is() {
        let mut app = app();
        let _ = app.update(Message::ShowAdd(None));
        let _ = app.update(Message::Address("ada@fastmail.com".into()));
        if let Page::Add(form) = &mut app.page {
            form.busy = Some(Busy::Checking);
        }

        let _ = app.update(Message::Added(Err(AddFailure::Exists("Fastmail".into()))));

        let form = form(&app);
        assert_eq!(form.busy, None);
        let error = form.error.as_deref().unwrap();
        assert!(error.contains("Fastmail"), "{error}");
        assert!(error.contains("already"), "{error}");
    }

    #[test]
    fn the_page_cannot_be_left_while_a_sign_in_is_under_way() {
        // Leaving would orphan the browser tab's answer: the account would be
        // stored with nothing on screen to say so.
        let mut app = app();
        let _ = app.update(Message::ShowAdd(None));
        if let Page::Add(form) = &mut app.page {
            form.busy = Some(Busy::SigningIn);
        }

        let _ = app.update(Message::ShowList);

        assert!(matches!(app.page, Page::Add(_)));
    }

    #[test]
    fn removing_asks_first() {
        let mut app = app();
        let account = Account::new("Fastmail", "", "ada@fastmail.com");
        let id = account.id.clone();
        app.accounts = vec![account];

        let _ = app.update(Message::RemoveRequested(id.clone()));
        assert_eq!(app.removing.as_deref(), Some(id.as_str()));
        let _ = app.update(Message::RemoveCancelled);

        assert_eq!(app.removing, None);
        assert_eq!(app.accounts.len(), 1);
    }

    #[test]
    fn a_launch_with_nothing_to_ask_only_raises_the_window() {
        use cosmic::app::CosmicFlags as _;

        assert!(Flags::new(Request::default()).action().is_none());
        let handed = Flags::new(Request {
            add: true,
            purpose: Some(Purpose::Contacts),
        });
        let action = handed.action().expect("an add request is handed over");
        let back: Request = serde_json::from_str(&action.to_string()).unwrap();
        assert_eq!(back.purpose, Some(Purpose::Contacts));
    }
}
