/*!
 Interactive form business payloads.

 Form requests and responses both store JSON in the archive's `data` field.
 Requests expose their display text through the template layout; responses also
 include submitted answers under `dynamic.selections`.
*/

use jzon::JsonValue;
use plist::Value;

use crate::{
    message_types::business_chat::{DYNAMIC_KEY, SELECTIONS_KEY},
    util::plist::get_string_from_dict,
};

/// One submitted answer group in a [`FormResponse`].
#[derive(Debug, PartialEq, Eq)]
pub struct FormAnswer {
    /// Prompt shown for this answer group.
    pub question: String,
    /// Submitted values, in display order.
    pub answers: Vec<String>,
}

/// Apple Business Chat form request.
///
/// The request can carry a blank form definition. The exporters only need the
/// template-layout title and subtitle.
#[derive(Debug, PartialEq, Eq)]
pub struct FormRequest {
    /// Template-layout title, for example `"Report an Issue"`.
    pub title: Option<String>,
    /// Template-layout subtitle, for example `"Tap to get started"`.
    pub subtitle: Option<String>,
}

/// Submitted Apple Business Chat form.
#[derive(Debug, PartialEq, Eq)]
pub struct FormResponse {
    /// Template-layout summary, for example `"Here's my completed form"`.
    pub summary: Option<String>,
    /// Submitted answer groups, in payload order.
    pub answers: Vec<FormAnswer>,
}

impl FormRequest {
    /// Extract a [`FormRequest`] from a decoded business JSON payload.
    ///
    /// The request body carries no display text; the title and subtitle come
    /// from the plist template layout, so `json` is unused.
    pub(super) fn from_json(_json: &JsonValue, payload: &Value) -> Self {
        let user_info = payload
            .as_dictionary()
            .and_then(|dict| dict.get("userInfo"));
        let title = user_info
            .and_then(|info| get_string_from_dict(info, "caption"))
            .or_else(|| get_string_from_dict(payload, "ldtext"))
            .map(str::to_string);
        let subtitle = user_info
            .and_then(|info| get_string_from_dict(info, "subcaption"))
            .map(str::to_string);

        FormRequest { title, subtitle }
    }
}

impl FormResponse {
    /// Extract a [`FormResponse`] from a decoded business JSON payload.
    ///
    /// The caller has already confirmed `dynamic.selections` is non-empty;
    /// `payload` supplies the plist-level `ldtext` summary.
    pub(super) fn from_json(json: &JsonValue, payload: &Value) -> Self {
        let answers = json[DYNAMIC_KEY][SELECTIONS_KEY]
            .members()
            .map(|page| FormAnswer {
                question: page["subtitle"].as_str().unwrap_or_default().to_string(),
                answers: page["items"]
                    .members()
                    .map(|item| item["title"].as_str().unwrap_or_default().to_string())
                    .collect(),
            })
            .collect();

        let summary = get_string_from_dict(payload, "ldtext").map(str::to_string);

        FormResponse { summary, answers }
    }
}

#[cfg(test)]
mod tests {
    use std::{env::current_dir, fs::File};

    use plist::Value;

    use crate::{
        message_types::{
            business_chat::{BusinessMessage, FormAnswer, FormRequest, FormResponse},
            variants::BalloonProvider,
        },
        util::plist::parse_ns_keyed_archiver,
    };

    fn archive(filename: &str) -> Value {
        let path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/app_message")
            .join(filename);
        let plist = Value::from_reader(File::open(path).unwrap()).unwrap();
        parse_ns_keyed_archiver(&plist).unwrap()
    }

    fn form_request(filename: &str) -> FormRequest {
        match BusinessMessage::from_map(&archive(filename)) {
            Ok(BusinessMessage::FormRequest(request)) => request,
            other => panic!("expected form request, got {other:?}"),
        }
    }

    fn form_response(filename: &str) -> FormResponse {
        match BusinessMessage::from_map(&archive(filename)) {
            Ok(BusinessMessage::FormResponse(response)) => response,
            other => panic!("expected form response, got {other:?}"),
        }
    }

    fn answer(question: &str, value: &str) -> FormAnswer {
        FormAnswer {
            question: question.to_string(),
            answers: vec![value.to_string()],
        }
    }

    #[test]
    fn test_parse_form_request() {
        assert_eq!(
            form_request("BusinessFormRequest.plist"),
            FormRequest {
                title: Some("Report an Issue".to_string()),
                subtitle: Some("Tap to get started".to_string()),
            }
        );
    }

    #[test]
    fn test_parse_form_response() {
        assert_eq!(
            form_response("BusinessFormResponse.plist"),
            FormResponse {
                summary: Some("Here's my completed form".to_string()),
                answers: vec![
                    answer(
                        "Which option best describes your request?",
                        "The first example option"
                    ),
                    answer("When did this happen?", "01/01/2024"),
                    answer("Anything else to add?", "Example free-text response."),
                ],
            }
        );
    }
}
