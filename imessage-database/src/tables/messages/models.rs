/*!
 Message body models reconstructed from [`message.attributed_body`](crate::tables::messages::message::Message::attributed_body).
*/

use std::fmt::{Display, Formatter, Result};

use crabstep::deserializer::iter::Property;

use crate::{
    message_types::text_effects::text_effect::TextEffect, tables::messages::message::Message,
};

// MARK: BubbleComponent
/// Component emitted for one logical message part.
///
/// # Component Types
///
/// A single iMessage contains data that may be represented across multiple bubbles.
/// Each bubble corresponds to one `__kIMMessagePartAttributeName` index in the
/// underlying [`NSAttributedString`](crate::util::typedstream); the
/// [`Run`](Self::Run) groups every attributed range that shares that part index.
#[derive(Debug, PartialEq, Clone)]
pub enum BubbleComponent {
    /// One bubble's worth of attributed body content. Each contained
    /// [`AttributedRange`] models a single `NSAttributedString` attribute run
    /// (a byte range plus its attribute dictionary); adjacent ranges that
    /// share a `__kIMMessagePartAttributeName` index share the bubble.
    ///
    /// A run may interleave text ranges ([`AttributedRange::attachment`] is
    /// `None`) with inline-attachment ranges (e.g. stickers rendered inline
    /// like emoji), preserving their original order.
    Run(Vec<AttributedRange>),
    /// An [app integration](crate::message_types::app)
    App,
    /// A component that was retracted, found by parsing the [`EditedMessage`](crate::message_types::edited::EditedMessage)
    Retracted,
}

// MARK: Service
/// Defines different types of [services](https://support.apple.com/en-us/104972) we can receive messages from.
#[derive(Debug, PartialEq, Eq)]
pub enum Service<'a> {
    /// iMessage.
    #[allow(non_camel_case_types)]
    iMessage,
    /// SMS.
    SMS,
    /// RCS.
    RCS,
    /// A message sent via [satellite](https://support.apple.com/en-us/120930) (literally: `iMessageLite` in the database).
    Satellite,
    /// Unrecognized service name.
    Other(&'a str),
    /// Missing service field.
    Unknown,
}

impl<'a> Service<'a> {
    /// Map the database service name to a [`Service`] variant.
    #[must_use]
    pub fn from_name(service: Option<&'a str>) -> Self {
        if let Some(service_name) = service {
            return match service_name.trim() {
                "iMessage" => Service::iMessage,
                "iMessageLite" => Service::Satellite,
                "SMS" => Service::SMS,
                "rcs" | "RCS" => Service::RCS,
                service_name => Service::Other(service_name),
            };
        }
        Service::Unknown
    }
}

impl Display for Service<'_> {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            Service::iMessage => write!(fmt, "iMessage"),
            Service::SMS => write!(fmt, "SMS"),
            Service::RCS => write!(fmt, "RCS"),
            Service::Satellite => write!(fmt, "Satellite"),
            Service::Other(other) => write!(fmt, "{other}"),
            Service::Unknown => write!(fmt, "Unknown"),
        }
    }
}

// MARK: AttributedRange
/// One attribute run of a message's [`NSAttributedString`](crate::util::typedstream)
/// body: a byte range into the [`Message`]'s [`text`](crate::tables::messages::Message::text)
/// plus every attribute applied to it.
///
/// A range is a *text* range when [`attachment`](Self::attachment) is `None` and
/// an *attachment* range (a `\u{FFFC}` placeholder for an inline attachment)
/// when it is `Some`. Effects, styles, and the inline-emoji hint apply to either
/// kind. The [`typedstream`](crate::util::typedstream) attribute dictionary is a flat bag, so an attachment
/// range can also carry, say, an [`Animated`](TextEffect::Animated) effect.
///
/// Ranges that share a `__kIMMessagePartAttributeName` index are grouped into one
/// [`BubbleComponent::Run`]. For example, message text with a
/// [`Mention`](TextEffect::Mention) like:
///
/// ```
/// let message_text = "What's up, Christopher?";
/// ```
///
/// parses into a single run of 3 ranges:
///
/// ```
/// use imessage_database::message_types::text_effects::text_effect::TextEffect;
/// use imessage_database::tables::messages::models::{AttributedRange, BubbleComponent};
///
/// let result = vec![BubbleComponent::Run(vec![
///     AttributedRange::text(0, 11, vec![TextEffect::Default]),  // `What's up, `
///     AttributedRange::text(11, 22, vec![TextEffect::Mention("+5558675309".to_string())]), // `Christopher`
///     AttributedRange::text(22, 23, vec![TextEffect::Default])  // `?`
/// ])];
/// ```
#[derive(Debug, PartialEq, Clone)]
pub struct AttributedRange {
    /// Start byte index in the message text.
    pub start: usize,
    /// End byte index in the message text.
    pub end: usize,
    /// Effects applied to this range.
    pub effects: Vec<TextEffect>,
    /// `Some` when this range is a `\u{FFFC}` placeholder for an attachment.
    /// The attachment's metadata travels here; effects still apply alongside.
    pub attachment: Option<AttachmentMeta>,
    /// `true` when the range carries `__kIMEmojiImageAttributeName`–Apple's
    /// hint to render the attachment inline–like an emoji (observed on
    /// genmoji, Memoji, and custom sticker ranges).
    pub emoji_image: bool,
}

impl AttributedRange {
    /// Build a text range (no attachment, no inline-emoji hint) with the
    /// specified start index, end index, and text effects.
    #[must_use]
    pub fn text(start: usize, end: usize, effects: Vec<TextEffect>) -> Self {
        Self {
            start,
            end,
            effects,
            attachment: None,
            emoji_image: false,
        }
    }

    /// Build an attachment range carrying the given [`AttachmentMeta`], with
    /// no inline-emoji hint.
    #[must_use]
    pub fn attachment(start: usize, end: usize, meta: AttachmentMeta) -> Self {
        Self {
            start,
            end,
            effects: vec![],
            attachment: Some(meta),
            emoji_image: false,
        }
    }

    /// Build an inline-rendered attachment range, one Apple flagged with
    /// `__kIMEmojiImageAttributeName` to render inline like an emoji (a Memoji,
    /// genmoji, or custom sticker placed amongst text).
    #[must_use]
    pub fn inline_attachment(start: usize, end: usize, meta: AttachmentMeta) -> Self {
        Self {
            start,
            end,
            effects: vec![],
            attachment: Some(meta),
            emoji_image: true,
        }
    }

    /// `true` when this range stands in for an attachment rather than text.
    #[must_use]
    pub fn is_attachment(&self) -> bool {
        self.attachment.is_some()
    }
}

// MARK: AttachmentMeta
/// Attachment metadata attached to a body range.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct AttachmentMeta {
    /// GUID of the attachment row.
    pub guid: Option<String>,
    /// Audio transcription stored on the attributed range.
    pub transcription: Option<String>,
    /// Inline media height in points.
    pub height: Option<f64>,
    /// Inline media width in points.
    pub width: Option<f64>,
    /// Original attachment filename.
    pub name: Option<String>,
}

impl AttachmentMeta {
    /// Applies a single typedstream attribute key/value pair to the metadata,
    /// ignoring any key that isn't attachment metadata. Driven per-key by the
    /// body parser's `build_range`, which walks the full attribute dictionary
    /// so non-attachment-meta keys on the same range are still processed.
    pub(crate) fn set_from_key_value<'a>(&mut self, key: &str, value: &Property<'a, 'a>) {
        match key {
            "__kIMFileTransferGUIDAttributeName" => {
                self.guid = value.as_string().map(String::from);
            }
            "IMAudioTranscription" => self.transcription = value.as_string().map(String::from),
            "__kIMInlineMediaHeightAttributeName" => self.height = value.as_f64(),
            "__kIMInlineMediaWidthAttributeName" => self.width = value.as_f64(),
            "__kIMFilenameAttributeName" => self.name = value.as_string().map(String::from),
            _ => {}
        }
    }
}

// MARK: SharedLocation
/// Direction of a legacy shared-location event (`item_type == 4` with
/// `group_action_type == 0`). The two cases are mutually exclusive: the
/// underlying `share_status` bool distinguishes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedLocation {
    /// The sender began sharing their location.
    Started,
    /// The sender stopped sharing their location.
    Stopped,
}

// MARK: GroupAction
/// Group action encoded by a message row.
#[derive(Debug, PartialEq, Eq)]
pub enum GroupAction<'a> {
    /// Participant was added to the group.
    ParticipantAdded(i32),
    /// Participant was removed from the group.
    ParticipantRemoved(i32),
    /// Group name changed.
    NameChange(&'a str),
    /// Participant left the group.
    ParticipantLeft,
    /// Group icon/avatar changed.
    GroupIconChanged,
    /// Group icon/avatar was removed.
    GroupIconRemoved,
    /// Chat background changed.
    ChatBackgroundChanged,
    /// Chat background was removed.
    ChatBackgroundRemoved,
    /// Participant changed their phone number.
    PhoneNumberChanged(i32),
}

impl<'a> GroupAction<'a> {
    /// Parse group action fields from a message row.
    #[must_use]
    pub(crate) fn from_message(message: &'a Message) -> Option<Self> {
        match (
            message.item_type,
            message.group_action_type,
            message.other_handle,
            &message.group_title,
        ) {
            // If the handle_id of the message matches the other_handle, the sender changed their own phone number
            (1, 0, Some(who), _) if message.handle_id == Some(who) => {
                Some(Self::PhoneNumberChanged(who))
            }
            (1, 0, Some(who), _) => Some(Self::ParticipantAdded(who)),
            (1, 1, Some(who), _) => Some(Self::ParticipantRemoved(who)),
            (2, _, _, Some(name)) => Some(Self::NameChange(name)),
            (3, 0, _, _) => Some(Self::ParticipantLeft),
            (3, 1, _, _) => Some(Self::GroupIconChanged),
            (3, 2, _, _) => Some(Self::GroupIconRemoved),
            (3, 4, _, _) => Some(Self::ChatBackgroundChanged),
            (3, 6, _, _) => Some(Self::ChatBackgroundRemoved),
            _ => None,
        }
    }
}
