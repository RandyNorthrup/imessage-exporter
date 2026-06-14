/*!
 Navigation helpers for Apple's `DDScannerResult` archives.

 These payloads are [`NSKeyedArchiver`](crate::util::plist) archives produced by
 the private `DataDetectorsCore` framework and stored inline in a message's
 attributed body (e.g. under `__kIMDataDetectedAttributeName`,
 `__kIMMoneyAttributeName`, or `__kIMAddressAttributeName`). Each archive
 describes a tree of *scanner results*; every node has a type
 ([`kind`](ScannerResult::kind)), an optional value ([`value`](ScannerResult::value)),
 the substring it matched ([`matched`](ScannerResult::matched)), and zero or more
 nested results ([`children`](ScannerResult::children)).

 [`ScannerResult`] is a lazy, borrowing cursor over one node of that tree. The
 semantic detector types parse themselves from a node via [`FromScannerResult`].
*/

use std::io::Cursor;

use crabstep::deserializer::iter::Property;
use plist::{Dictionary, Value};

/// Maximum scanner-result depth before traversal stops.
///
/// `NSKeyedArchiver` graphs are deduplicated by `UID` and may contain reference
/// cycles, so recursion is bounded for malformed payloads.
const MAX_DEPTH: usize = 8;

/// Borrowing, lazily-resolved cursor over one `DDScannerResult` tree node.
///
/// Fields are stored as `UID` indices into the archive's `$objects` table and
/// resolved on access, so constructing or walking a `ScannerResult` allocates
/// nothing beyond the child-index list produced by [`children`](Self::children).
#[derive(Clone, Copy)]
pub struct ScannerResult<'a> {
    /// The archive's `$objects` table; every field is a `UID` index into this.
    objects: &'a [Value],
    /// The index of this node within `objects`.
    index: usize,
    /// How deep this node sits in the tree, used to bound recursion.
    depth: usize,
}

impl<'a> ScannerResult<'a> {
    /// Resolve the root scanner result from a parsed detector archive.
    ///
    /// The root index is stored under `$top.dd-result` (falling back to
    /// `$top.root`) and points into the archive's `$objects` table.
    #[must_use]
    pub fn root(plist: &'a Value) -> Option<Self> {
        let body = plist.as_dictionary()?;
        let objects = body.get("$objects")?.as_array()?;
        let top = body.get("$top")?.as_dictionary()?;
        let index = top
            .get("dd-result")
            .or_else(|| top.get("root"))
            .and_then(uid_index)?;
        Some(Self {
            objects,
            index,
            depth: 0,
        })
    }

    /// The result type from the `T` field (e.g. `"Money"`, `"Unit"`, `"TrackingNumber"`).
    #[must_use]
    pub fn kind(&self) -> Option<&'a str> {
        self.field_string("T")
    }

    /// The result value from the `V` field, if present.
    #[must_use]
    pub fn value(&self) -> Option<&'a str> {
        self.field_string("V")
    }

    /// The substring of the message text this result matched from the `MS` field.
    #[must_use]
    pub fn matched(&self) -> Option<&'a str> {
        self.field_string("MS")
    }

    /// Child results from the `SR` array, depth-bounded so cyclic archives
    /// terminate.
    pub fn children(&self) -> impl Iterator<Item = ScannerResult<'a>> + '_ {
        self.child_indices()
            .unwrap_or_default()
            .into_iter()
            .map(|index| ScannerResult {
                objects: self.objects,
                index,
                depth: self.depth + 1,
            })
    }

    /// The dictionary backing this node.
    fn dict(&self) -> Option<&'a Dictionary> {
        self.objects.get(self.index)?.as_dictionary()
    }

    /// Resolve a `UID`-referenced string field by key.
    fn field_string(&self, key: &str) -> Option<&'a str> {
        let reference = self.dict()?.get(key)?;
        self.objects.get(uid_index(reference)?)?.as_string()
    }

    /// Resolve the `SR` array to the object indices of its child results, or
    /// `None` once the depth bound is reached.
    fn child_indices(&self) -> Option<Vec<usize>> {
        if self.depth >= MAX_DEPTH {
            return None;
        }
        let sub_results = self.dict()?.get("SR")?;
        let array = self
            .objects
            .get(uid_index(sub_results)?)?
            .as_dictionary()?
            .get("NS.objects")?
            .as_array()?;
        Some(array.iter().filter_map(uid_index).collect())
    }
}

/// Type that can recognize itself from a [`ScannerResult`] node.
///
/// Returning `None` means "this node is not of the implementing type," which is
/// an expected outcome rather than an error.
pub trait FromScannerResult: Sized {
    /// Byte markers used to reject impossible payloads before plist parsing.
    ///
    /// When non-empty, [`from_attribute`](Self::from_attribute) parses the
    /// payload only if it contains at least one of these byte sequences. This
    /// skips deserializing results from the shared `__kIMDataDetectedAttributeName`
    /// attribute that cannot be `Self`, since that attribute carries every
    /// data-detector type. Types parsed from a dedicated attribute leave this
    /// empty (the default).
    const MARKERS: &[&[u8]] = &[];

    /// Parse `Self` from a scanner-result node, or return `None` on mismatch.
    fn from_scanner_result(result: &ScannerResult<'_>) -> Option<Self>;

    /// Parse `Self` from a typedstream attribute carrying a `DDScannerResult`
    /// archive (`NSData` or `NSMutableData`).
    ///
    /// Returns `None` when the value is not data, fails the
    /// [`MARKERS`](Self::MARKERS) pre-filter, is not a valid archive, or does
    /// not represent a `Self`.
    fn from_attribute<'p>(value: &Property<'p, 'p>) -> Option<Self> {
        let data = value.as_data()?;
        if !Self::MARKERS.is_empty()
            && !Self::MARKERS
                .iter()
                .any(|marker| data.windows(marker.len()).any(|window| window == *marker))
        {
            return None;
        }
        let plist = Value::from_reader(Cursor::new(data)).ok()?;
        Self::from_scanner_result(&ScannerResult::root(&plist)?)
    }
}

/// Interpret a plist `UID` as an index into the `$objects` table.
fn uid_index(value: &Value) -> Option<usize> {
    Some(value.as_uid()?.get() as usize)
}
