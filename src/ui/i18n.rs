/// Internationalization infrastructure for the UI. Builds heavily on Fluent
use fluent::*;
use maplit::*;
use std::borrow::Cow;
use unic_langid::{langid, LanguageIdentifier};

/// A language description.
pub struct Language {
    bundle: FluentBundle<FluentResource>,
}

impl Language {
    /// Construct a language from a language identifier and a fluent file
    /// defining translations.
    pub fn new(lang: LanguageIdentifier, ftl: &'static str) -> Self {
        let res = FluentResource::try_new(ftl.to_owned()).expect("Failed to parse FTL");
        let mut bundle = FluentBundle::new(&[lang]);

        bundle
            .add_resource(res)
            .expect("Failed to add FTL resources");

        Self { bundle }
    }

    /// Construct from only a language identifier, using the stored FTL data
    /// included at compile time.
    ///
    /// This is how languages should typically be constructed.
    pub fn from_langid(lang: LanguageIdentifier) -> Self {
        let langs = hashmap! {
            langid!("en-US") => include_str!("../../i18n/en-US.ftl"),
            langid!("de-DE") => include_str!("../../i18n/de-DE.ftl")
        };

        Self::new(
            lang.clone(),
            *langs
                .get(&lang)
                .or_else(|| langs.get(&langid!("en-US")))
                .unwrap(),
        )
    }

    /// Translate a message from a placeholder string using this language.
    pub fn get_message(&self, id: &str) -> Cow<str> {
        if let Some(msg) = self.bundle.get_message(id) {
            let pattern = msg.value.expect("Message without value");
            let mut errors = vec![];
            self.bundle.format_pattern(pattern, None, &mut errors)
        } else {
            Cow::Owned(id.to_owned())
        }
    }
}

impl Default for Language {
    /// The default language is en-US
    fn default() -> Self {
        Self::from_langid(langid!("en-US"))
    }
}
