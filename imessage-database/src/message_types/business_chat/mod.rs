/*!
 Apple Business Chat payload parsers.

 The Messages business extension uses
 `com.apple.icloud.apps.messages.business.extension` for several interactive
 schemas. [`BusinessMessage`] inspects the decoded payload and chooses the
 parser that matches the schema.
*/

mod business;
mod form;
mod list_picker;
mod quick_reply;

pub use business::BusinessMessage;
pub use form::{FormAnswer, FormRequest, FormResponse};
pub use list_picker::{ListPicker, ListPickerItem};
pub use quick_reply::{QuickReply, QuickReplyOption};

/// Top-level keys that discriminate the business JSON schemas. The bundle ID
/// routes every interactive business payload into this module; these keys pick
/// the concrete schema from the decoded JSON.
const QUICK_REPLY_KEY: &str = "quick-reply";
const LIST_PICKER_KEY: &str = "listPicker";
const DYNAMIC_KEY: &str = "dynamic";
const SELECTIONS_KEY: &str = "selections";
