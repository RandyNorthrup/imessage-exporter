/*!
 Helpers for working with `Property` types in the [Crabstep](https://github.com/ReagentX/crabstep) deserializer.
*/

use crabstep::{OutputData, deserializer::iter::Property};

/// Pair used by attributed-body ranges: attribute dictionary index plus UTF-16
/// range length.
#[derive(Debug)]
pub struct TypeLengthPair {
    /// Index of the attribute dictionary referenced by this range.
    pub type_index: i64,
    /// Length of the range in UTF-16 code units.
    pub length: u64,
}

// MARK: Type Length
/// Extract a [`TypeLengthPair`] from a two-integer typedstream group.
///
/// No Foundation accessor reads "two primitives in a group", so this stays on
/// the generic [`Property`]/[`OutputData`] API.
#[inline(always)]
pub fn as_type_length_pair(property: &Property<'_, '_>) -> Option<TypeLengthPair> {
    if let Property::Group(group) = property {
        let mut iter = group.iter();
        if let Some(Property::Primitive(OutputData::SignedInteger(type_index))) = iter.next()
            && let Some(Property::Primitive(OutputData::UnsignedInteger(length))) = iter.next()
        {
            return Some(TypeLengthPair {
                type_index: *type_index,
                length: *length,
            });
        }
    }
    None
}
