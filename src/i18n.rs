// SPDX-License-Identifier: GPL-3.0-only

//! Provides localization support for this crate.

use i18n_embed::fluent::{FluentLanguageLoader, fluent_language_loader};
use i18n_embed::unic_langid::LanguageIdentifier;
use i18n_embed::{DefaultLocalizer, LanguageLoader, Localizer};
use rust_embed::RustEmbed;
use std::sync::LazyLock;

/// Applies the requested language(s) to translations from the `fl!()` macro.
pub fn init(requested_languages: &[LanguageIdentifier]) {
    if let Err(why) = localizer().select(requested_languages) {
        eprintln!("error while loading fluent localizations: {why}");
    }
}

#[must_use]
pub fn localizer() -> Box<dyn Localizer> {
    Box::from(DefaultLocalizer::new(&*LANGUAGE_LOADER, &Localizations))
}

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

pub static LANGUAGE_LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let loader: FluentLanguageLoader = fluent_language_loader!();
    loader
        .load_fallback_language(&Localizations)
        .expect("Error while loading fallback language");
    loader
});

/// Request a localized string by ID from the `i18n/` directory.
#[macro_export]
macro_rules! fl {
    ($message_id:literal) => {{
        i18n_embed_fl::fl!($crate::i18n::LANGUAGE_LOADER, $message_id)
    }};

    ($message_id:literal, $($args:expr),*) => {{
        i18n_embed_fl::fl!($crate::i18n::LANGUAGE_LOADER, $message_id, $($args), *)
    }};
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    /// Every message id in a Fluent file, with the variables it uses.
    fn messages(ftl: &str) -> BTreeMap<String, BTreeSet<String>> {
        let mut messages = BTreeMap::new();
        let mut current: Option<String> = None;
        for line in ftl.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            if !line.starts_with(' ')
                && let Some((id, _)) = line.split_once(" =")
            {
                current = Some(id.trim().to_owned());
                messages.insert(id.trim().to_owned(), BTreeSet::new());
            }
            if let Some(id) = &current {
                let variables = messages.get_mut(id).expect("inserted above");
                for rest in line.split("{ $").skip(1) {
                    let name: String = rest
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                        .collect();
                    variables.insert(name);
                }
            }
        }
        messages
    }

    #[test]
    fn the_greek_translation_has_every_message_with_the_same_variables() {
        let english = messages(include_str!("../i18n/en/magnetar_accounts.ftl"));
        let greek = messages(include_str!("../i18n/el/magnetar_accounts.ftl"));

        assert_eq!(
            english.keys().collect::<Vec<_>>(),
            greek.keys().collect::<Vec<_>>(),
            "the two languages name different messages"
        );
        for (id, variables) in &english {
            assert_eq!(&greek[id], variables, "{id} uses different variables");
        }
    }
}
