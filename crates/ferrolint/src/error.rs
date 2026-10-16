use std::fmt;

/// What stopped a run from completing; every kind exits with code 2.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Usage,
    Config,
    Plugin,
    Io,
}

/// A failure that stops the run before it can report.
#[derive(Debug)]
pub struct Error {
    pub kind: Kind,
    pub message: String,
}

impl Error {
    pub fn usage(message: impl Into<String>) -> Self {
        Self {
            kind: Kind::Usage,
            message: message.into(),
        }
    }

    pub fn config(message: impl Into<String>) -> Self {
        Self {
            kind: Kind::Config,
            message: message.into(),
        }
    }

    pub fn plugin(message: impl Into<String>) -> Self {
        Self {
            kind: Kind::Plugin,
            message: message.into(),
        }
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self {
            kind: Kind::Io,
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.kind {
            Kind::Usage => "usage error",
            Kind::Config => "config error",
            Kind::Plugin => "plugin error",
            Kind::Io => "i/o error",
        };
        write!(f, "{kind}: {}", self.message)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
