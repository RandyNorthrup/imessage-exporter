/*!
 Quick reply business payloads.

 Quick replies store JSON in the archive's `data` field. Prompt payloads list
 the available options. Reply payloads add `selectedIndex`.
*/

use jzon::JsonValue;
use plist::Value;

use crate::{message_types::business_chat::QUICK_REPLY_KEY, util::plist::get_string_from_dict};

/// One option in a [`QuickReply`] prompt.
#[derive(Debug, PartialEq, Eq)]
pub struct QuickReplyOption {
    /// Display title, for example `"Yes"`.
    pub title: String,
}

/// Apple Business Chat quick reply.
///
/// The same model represents prompts and replies. Prompts have
/// [`options`](Self::options) and no [`selected_index`](Self::selected_index);
/// replies carry the same option list plus a selected index.
#[derive(Debug, PartialEq, Eq)]
pub struct QuickReply {
    /// Prompt summary or template-layout fallback text.
    pub summary: Option<String>,
    /// Options in display order.
    pub options: Vec<QuickReplyOption>,
    /// Index into [`options`](Self::options) selected by a reply.
    pub selected_index: Option<usize>,
}

impl QuickReply {
    /// Extract a [`QuickReply`] from a decoded business JSON payload.
    ///
    /// The caller has already matched the `quick-reply` schema; `payload`
    /// supplies the plist-level `ldtext` fallback.
    pub(super) fn from_json(json: &JsonValue, payload: &Value) -> Self {
        let quick_reply = &json[QUICK_REPLY_KEY];

        let options = quick_reply["items"]
            .members()
            .map(|item| QuickReplyOption {
                title: item["title"].as_str().unwrap_or_default().to_string(),
            })
            .collect();

        let summary = quick_reply["summaryText"]
            .as_str()
            .map(str::to_string)
            .or_else(|| get_string_from_dict(payload, "ldtext").map(str::to_string));

        QuickReply {
            summary,
            options,
            selected_index: quick_reply["selectedIndex"].as_usize(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{env::current_dir, fs::File};

    use plist::Value;

    use crate::{
        message_types::{
            business_chat::{BusinessMessage, QuickReply, QuickReplyOption},
            variants::BalloonProvider,
        },
        util::plist::parse_ns_keyed_archiver,
    };

    fn parse(filename: &str) -> QuickReply {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/app_message")
            .join(filename);
        let plist = Value::from_reader(File::open(plist_path).unwrap()).unwrap();
        match BusinessMessage::from_map(&parse_ns_keyed_archiver(&plist).unwrap()) {
            Ok(BusinessMessage::QuickReply(quick_reply)) => quick_reply,
            other => panic!("expected quick reply, got {other:?}"),
        }
    }

    fn option(title: &str) -> QuickReplyOption {
        QuickReplyOption {
            title: title.to_string(),
        }
    }

    #[test]
    fn test_parse_business_quick_reply_prompt() {
        let balloon = parse("BusinessQuickReply.plist");
        let expected = QuickReply {
            summary: Some("Choose an option".to_string()),
            options: vec![option("Yes"), option("No")],
            selected_index: None,
        };
        assert_eq!(balloon, expected);
    }

    #[test]
    fn test_parse_business_quick_reply_response() {
        let balloon = parse("BusinessQuickReplyResponse.plist");
        let expected = QuickReply {
            // Replies in this fixture have no `summaryText`; `ldtext` is the fallback.
            summary: Some("Replied to a question".to_string()),
            options: vec![option("Yes"), option("No")],
            selected_index: Some(0),
        };
        assert_eq!(balloon, expected);
    }
}
