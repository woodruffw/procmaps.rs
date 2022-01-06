//! Error types for `rsprocmaps`.

use std::error;
use std::fmt;
use std::io;
use std::num;

use pest::error::Error as PestError;

use crate::Rule;

/// An enumeration of possible error states for `rsprocmaps`.
#[derive(Debug)]
pub enum Error {
    /// An I/O error.
    Io(io::Error),
    /// A general parsing error.
    ParseError(PestError<Rule>),
    // NOTE(ww): ParseIntError is more general than just numbers that don't
    // fit into a particular width, but we handle all of its other parsing issues
    // at the pest/actual parsing level.
    /// An integer-width parsing error.
    WidthError(num::ParseIntError),
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::Io(err)
    }
}

impl From<PestError<Rule>> for Error {
    fn from(err: PestError<Rule>) -> Error {
        Error::ParseError(err)
    }
}

impl From<num::ParseIntError> for Error {
    fn from(err: num::ParseIntError) -> Error {
        Error::WidthError(err)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Io(ref e) => e.fmt(f),
            Error::ParseError(ref e) => e.fmt(f),
            Error::WidthError(ref e) => e.fmt(f),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            Error::Io(ref e) => Some(e),
            Error::ParseError(ref e) => Some(e),
            Error::WidthError(ref e) => Some(e),
        }
    }
}
