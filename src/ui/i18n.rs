use fluent::*;
use maplit::*;
use std::borrow::Cow;
use unic_langid::{langid, LanguageIdentifier};

pub struct Language {
    bundle: FluentBundle<FluentResource>,
}

impl Language {
    pub fn new(lang: LanguageIdentifier, ftl: &'static str) -> Self {
        let res = FluentResource::try_new(ftl.to_owned()).expect("Failed to parse FTL");
        let mut bundle = FluentBundle::new(&[lang]);

        bundle
            .add_resource(res)
            .expect("Failed to add FTL resources");

        Self { bundle }
    }

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
    fn default() -> Self {
        Self::from_langid(langid!("en-US"))
    }
}
