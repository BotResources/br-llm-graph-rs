use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Site(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a site has a non-blank text")]
pub struct InvalidSite;

impl Site {
    pub fn new(text: impl Into<String>) -> Result<Self, InvalidSite> {
        let text = text.into();
        if text.trim().is_empty() {
            return Err(InvalidSite);
        }
        Ok(Self(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Site {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebAccess {
    Full,
    Whitelist(Vec<Site>),
    None,
}

impl WebAccess {
    pub fn reaches_nothing(&self) -> bool {
        matches!(self, WebAccess::None)
    }

    pub fn sites(&self) -> &[Site] {
        match self {
            WebAccess::Whitelist(sites) => sites,
            WebAccess::Full | WebAccess::None => &[],
        }
    }
}
