//! The wire: HTTPS to `api.telegram.org`, and every possible answer — a
//! refusal, a dead network, a wrong token — turned into one short Persian
//! sentence a shopkeeper can act on.
//!
//! The token is the shopkeeper's secret. It lives here only long enough to be
//! put in the URL, and it is never allowed into a message, an error or a file
//! name. Nothing here formats the engine's own text, because the engine's text
//! can quote the URL, and the URL carries the token.

use crate::db::Res;
use serde_json::Value;
use std::time::Duration;

/// Where a Telegram call goes.
///
/// The real one goes over HTTPS. The tests use a recording stub, so no test in
/// this crate ever touches the network — the shop's phone may be offline, and a
/// test suite that needs Telegram would be a test suite nobody runs.
pub trait Transport {
    /// `getMe`, `getUpdates`: a JSON body in, Telegram's full answer out —
    /// `{"ok":true,"result":…}` or `{"ok":false,"description":…}`.
    fn post_json(&self, method: &str, body: &Value) -> Res<Value>;

    /// `sendDocument`: the text parts plus one file, framed as multipart.
    fn post_file(
        &self,
        method: &str,
        fields: &[(&str, &str)],
        file_field: &str,
        file_name: &str,
        bytes: &[u8],
    ) -> Res<Value>;
}

/// The real transport: one blocking HTTPS client, 30 s end to end.
pub struct Https {
    token: String,
    agent: ureq::Agent,
}

impl Https {
    pub fn new(token: &str) -> Self {
        let config = ureq::Agent::config_builder()
            // Telegram refuses with a JSON body *and* a 4xx status. We want to
            // read that body, so a status code is data here, not an error.
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build();
        Self {
            token: token.trim().to_string(),
            agent: ureq::Agent::new_with_config(config),
        }
    }

    fn url(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{}", self.token, method)
    }
}

impl Transport for Https {
    fn post_json(&self, method: &str, body: &Value) -> Res<Value> {
        let mut answer = self
            .agent
            .post(self.url(method))
            .send_json(body)
            .map_err(net_error)?;
        answer
            .body_mut()
            .read_json::<Value>()
            .map_err(|_| "پاسخ تلگرام خوانده نشد".to_string())
    }

    fn post_file(
        &self,
        method: &str,
        fields: &[(&str, &str)],
        file_field: &str,
        file_name: &str,
        bytes: &[u8],
    ) -> Res<Value> {
        let part = crate::telegram::multipart::build(
            &crate::telegram::multipart::boundary(),
            fields,
            file_field,
            file_name,
            bytes,
        );
        let mut answer = self
            .agent
            .post(self.url(method))
            .content_type(part.content_type.as_str())
            .send(&part.body)
            .map_err(net_error)?;
        answer
            .body_mut()
            .read_json::<Value>()
            .map_err(|_| "پاسخ تلگرام خوانده نشد".to_string())
    }
}

/// A call that is expected to succeed: the envelope is unwrapped for you.
pub fn call_json(t: &dyn Transport, method: &str, body: &Value) -> Res<Value> {
    let answer = t.post_json(method, body)?;
    unwrap_answer(&answer)
}

/// An upload that is expected to succeed: the envelope is unwrapped for you.
pub fn send_file(
    t: &dyn Transport,
    method: &str,
    fields: &[(&str, &str)],
    file_field: &str,
    file_name: &str,
    bytes: &[u8],
) -> Res<()> {
    let answer = t.post_file(method, fields, file_field, file_name, bytes)?;
    unwrap_answer(&answer).map(|_| ())
}

/// `{"ok":true,"result":…}` is success; anything else is a refusal to explain.
pub fn unwrap_answer(answer: &Value) -> Res<Value> {
    if answer.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok(answer.get("result").cloned().unwrap_or(Value::Null));
    }
    let code = answer
        .get("error_code")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let description = answer
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    Err(refusal(code, description))
}

/// Telegram's English reason, mapped onto something the shopkeeper can act on.
///
/// The text Telegram gave us is read, never repeated: it is the one piece of
/// foreign text in this app, and the one place an echoed token could hide.
pub fn refusal(code: i64, description: &str) -> String {
    let said = description.to_ascii_lowercase();
    if code == 403
        || said.contains("chat not found")
        || said.contains("blocked")
        || said.contains("kicked")
    {
        return "ربات به گفتگو دسترسی ندارد".to_string();
    }
    if code == 401 || said.contains("unauthorized") || said.contains("token") {
        return "توکن ربات درست نیست".to_string();
    }
    if code == 429 || said.contains("too many requests") {
        return "تلگرام شلوغ است، کمی بعد دوباره بزنید".to_string();
    }
    if said.contains("caption") && said.contains("long") {
        return "متن فاکتور بلندتر از حد تلگرام است".to_string();
    }
    "تلگرام درخواست را نپذیرفت".to_string()
}

/// No wording from the engine reaches the shopkeeper — and nothing here can
/// carry the token, because the URL is never printed.
fn net_error(e: ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(code) => match code {
            401 => "توکن ربات درست نیست".to_string(),
            403 => "ربات به گفتگو دسترسی ندارد".to_string(),
            429 => "تلگرام شلوغ است، کمی بعد دوباره بزنید".to_string(),
            _ => "به تلگرام وصل نشد".to_string(),
        },
        ureq::Error::Timeout(_)
        | ureq::Error::Io(_)
        | ureq::Error::ConnectionFailed
        | ureq::Error::HostNotFound => "اینترنت وصل نیست".to_string(),
        _ => "به تلگرام وصل نشد".to_string(),
    }
}

/// Belt and braces: whatever text comes back, it leaves here without the token
/// in it. Callers wrap every message they hand to the interface in this.
pub fn scrub(text: &str, token: &str) -> String {
    let token = token.trim();
    if token.is_empty() {
        return text.to_string();
    }
    text.replace(token, "…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telegram::TEST_TOKEN as TOKEN;
    use serde_json::json;

    #[test]
    fn a_good_answer_hands_back_the_result() {
        let answer = json!({ "ok": true, "result": { "id": 42, "username": "daftar_bot" } });
        let result = unwrap_answer(&answer).unwrap();
        assert_eq!(result["username"], "daftar_bot");
    }

    #[test]
    fn every_refusal_is_a_persian_sentence_without_the_token() {
        let refusals = [
            (401, "Unauthorized"),
            (400, "Bad Request: chat not found"),
            (403, "Forbidden: bot was blocked by the user"),
            (429, "Too Many Requests: retry after 5"),
            (400, "Bad Request: message caption is too long"),
            (500, "Internal Server Error"),
            // A description that echoes the token must not come back out.
            (400, "Bad Request: nope"),
        ];
        for (code, description) in refusals {
            let answer = json!({
                "ok": false,
                "error_code": code,
                "description": format!("{description} {TOKEN}"),
            });
            let message = unwrap_answer(&answer).unwrap_err();
            assert!(!message.contains(TOKEN), "token leaked in: {message}");
            assert!(
                message.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c)),
                "not Persian: {message}"
            );
            assert!(!message.contains("Bad Request"), "english leaked: {message}");
        }
    }

    #[test]
    fn scrub_takes_the_token_out_of_anything() {
        assert_eq!(scrub("خطا", TOKEN), "خطا");
        assert_eq!(
            scrub(&format!("اینترنت وصل نیست {TOKEN}"), TOKEN),
            "اینترنت وصل نیست …"
        );
        assert_eq!(scrub("خطا", ""), "خطا");
    }
}
