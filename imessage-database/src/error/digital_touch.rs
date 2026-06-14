/*!
 Errors that can happen when parsing `digital touch` data.
*/

use std::fmt::{Display, Formatter, Result};

/// Errors that can happen when parsing [`digital touch`](crate::message_types::digital_touch) data.
#[derive(Debug)]
pub enum DigitalTouchError {
    /// Wraps an error returned by the protobuf parser.
    ProtobufError(protobuf::Error),
    /// The `TouchKind` discriminant was not a value we know how to parse.
    UnknownDigitalTouchKind(i32),
    /// Two parallel arrays that are expected to describe the same events had
    /// different lengths (name, length, other name, other length).
    ArraysDoNotMatch(&'static str, usize, &'static str, usize),
    /// A length-prefixed stroke ran past the end of its buffer (needed, available).
    InvalidStrokesLength(usize, usize),
    /// Wraps an error returned while reading an embedded `NSKeyedArchiver` archive.
    ArchiveError(plist::Error),
}

impl std::error::Error for DigitalTouchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DigitalTouchError::ProtobufError(why) => Some(why),
            DigitalTouchError::ArchiveError(why) => Some(why),
            _ => None,
        }
    }
}

impl Display for DigitalTouchError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            DigitalTouchError::ProtobufError(why) => {
                write!(fmt, "failed to parse digital touch protobuf: {why}")
            }
            DigitalTouchError::UnknownDigitalTouchKind(kind) => {
                write!(fmt, "unknown digital touch kind: {kind}")
            }
            DigitalTouchError::ArraysDoNotMatch(n1, v1, n2, v2) => {
                write!(fmt, "mismatched array lengths: {n1} ({v1}) != {n2} ({v2})")
            }
            DigitalTouchError::InvalidStrokesLength(needed, available) => {
                write!(
                    fmt,
                    "stroke needs {needed} bytes but only {available} remain"
                )
            }
            DigitalTouchError::ArchiveError(why) => {
                write!(fmt, "failed to read digital touch media archive: {why}")
            }
        }
    }
}
