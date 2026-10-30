//! The Telegram bot: invoices, statements and backups to the shopkeeper's own
//! chat with his bot.
//!
//! One chat, one direction. He pastes a token, presses Start once in the bot,
//! and that Start message tells us the chat. Everything afterwards is uploaded
//! there and he forwards it to whoever he wants — the app never messages a
//! customer, and it never asks him to leave the shop screen to do admin.

pub mod client;
pub mod multipart;

use crate::db::Res;
use crate::dto::BotInfo;
use crate::Ctx;
use client::Transport;
use rusqlite::OptionalExtension;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// How long `telegram.link` waits for the shopkeeper to press Start on his bot.
const WAIT: Duration = Duration::from_secs(25);
/// Between two looks. Telegram is polled, not hammered.
const LOOK: Duration = Duration::from_millis(1000);

/// The sentence every Telegram op falls back to before the bot is wired up.
/// It names the screen, because a man holding a red toast needs to know where
/// to go, not what is wrong.
const FIRST_WIRE: &str = "ربات تلگرام وصل نیست؛ از تنظیمات وصلش کن";

pub fn handle(ctx: &Ctx<'_>, op: &str, args: &Value) -> Option<Res<Value>> {
    Some(match op {
        // Read-only check: what is this token, and has he already pressed Start?
        "telegram.check" => (|| {
            let token = token_arg(args)?;
            let t = client::Https::new(&token);
            let info = guarded(check_with(ctx, &t), &token)?;
            json_of(info)
        })(),

        // The one onboarding step. Waits for the Start message, then saves.
        "telegram.link" => (|| {
            let token = token_arg(args)?;
            let t = client::Https::new(&token);
            let info = guarded(link_with(ctx, &t, &token), &token)?;
            json_of(info)
        })(),

        "invoice.send" => (|| {
            let doc_id = args
                .get("docId")
                .and_then(Value::as_i64)
                .ok_or_else(|| "فاکتور پیدا نشد".to_string())?;
            let (token, chat, shop) = wire(ctx)?;
            if token.is_empty() || chat.is_empty() {
                return Ok(said(false, FIRST_WIRE));
            }
            let caption = match doc_caption(ctx, doc_id, &shop) {
                Ok(caption) => caption,
                Err(e) => return Ok(said(false, &e)),
            };
            let rendered = match crate::invoice::render_doc(ctx, doc_id) {
                Ok(rendered) => rendered,
                Err(e) => return Ok(said(false, &e)),
            };
            let bytes = match std::fs::read(&rendered.pdf_path) {
                Ok(bytes) => bytes,
                Err(_) => return Ok(said(false, "فایل فاکتور پیدا نشد")),
            };
            let t = client::Https::new(&token);
            let fields = [("chat_id", chat.as_str()), ("caption", caption.as_str())];
            match client::send_file(
                &t,
                "sendDocument",
                &fields,
                "document",
                &rendered.file_name,
                &bytes,
            ) {
                // The next thing he has to do is forward this to his customer,
                // and nothing else on the screen will tell him.
                Ok(()) => Ok(said(true, "فاکتور به تلگرام رفت؛ برای مشتری بفرستش")),
                Err(e) => Ok(said(false, &client::scrub(&e, &token))),
            }
        })(),

        "invoice.sendStatement" => (|| {
            let party_id = args
                .get("partyId")
                .and_then(Value::as_i64)
                .ok_or_else(|| "طرف حساب پیدا نشد".to_string())?;
            let (token, chat, shop) = wire(ctx)?;
            if token.is_empty() || chat.is_empty() {
                return Ok(said(false, FIRST_WIRE));
            }
            let caption = match statement_caption(ctx, party_id, &shop) {
                Ok(caption) => caption,
                Err(e) => return Ok(said(false, &e)),
            };
            let rendered = match crate::invoice::render_statement(ctx, party_id) {
                Ok(rendered) => rendered,
                Err(e) => return Ok(said(false, &e)),
            };
            let bytes = match std::fs::read(&rendered.pdf_path) {
                Ok(bytes) => bytes,
                Err(_) => return Ok(said(false, "فایل صورت‌حساب پیدا نشد")),
            };
            let t = client::Https::new(&token);
            let fields = [("chat_id", chat.as_str()), ("caption", caption.as_str())];
            match client::send_file(
                &t,
                "sendDocument",
                &fields,
                "document",
                &rendered.file_name,
                &bytes,
            ) {
                Ok(()) => Ok(said(true, "صورت‌حساب به تلگرام رفت؛ برای مشتری بفرستش")),
                Err(e) => Ok(said(false, &client::scrub(&e, &token))),
            }
        })(),

        _ => return None,
    })
}

// ---------------------------------------------------------------- the two checks

/// Answers "which bot is this, and does it already know a chat?".
///
/// Nothing is saved: this is the screen where he is still typing the token, and
/// a typo must not end up in the settings table.
pub fn check_with(ctx: &Ctx<'_>, t: &dyn Transport) -> Res<BotInfo> {
    let (username, name) = me_info(t)?;

    // A chat is a bonus here, not the point of the call: if Telegram will not
    // tell us about updates right now, the token is still proven good.
    let found = client::call_json(t, "getUpdates", &json!({ "limit": 100 }))
        .ok()
        .and_then(|updates| newest_chat(&updates));

    let (chat_id, chat_title) = match found {
        Some((id, title)) => (Some(id), title),
        None => match saved_chat(ctx) {
            Some(id) => (Some(id), None),
            None => (None, None),
        },
    };

    Ok(BotInfo {
        username,
        name,
        chat_id,
        chat_title,
    })
}

/// The onboarding step: wait for him to press Start, take that chat, save it.
///
/// Telegram keeps undelivered updates for a day, so the very first look already
/// sees a Start he pressed a moment ago. If his Start arrived before this call,
/// we link straight away; if it arrives during the wait, the next look catches
/// it.
pub fn link_with(ctx: &Ctx<'_>, t: &dyn Transport, token: &str) -> Res<BotInfo> {
    let (username, name) = me_info(t)?;
    let mut offset = saved_offset(ctx);
    let deadline = Instant::now() + WAIT;

    loop {
        let mut body = json!({ "limit": 100, "timeout": 0 });
        if let Some(next) = offset {
            body["offset"] = json!(next);
        }
        let updates = client::call_json(t, "getUpdates", &body)?;

        if let Some((chat_id, chat_title)) = newest_chat(&updates) {
            save_link(ctx, token, &chat_id, highest_update_id(&updates))?;
            return Ok(BotInfo {
                username,
                name,
                chat_id: Some(chat_id),
                chat_title,
            });
        }

        // Nothing new: remember where we got to, so the next look — and the
        // next time he presses the button — does not replay old messages.
        if let Some(last) = highest_update_id(&updates) {
            offset = Some(last + 1);
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(LOOK);
    }

    // The token was good enough to find the bot, so say that first: a man who
    // has just pasted a long code needs to know the code is not the problem.
    Err("ربات پیدا شد؛ فقط Start کم است".to_string())
}

/// Who the token belongs to: `getMe` in, `(username, name)` out.
fn me_info(t: &dyn Transport) -> Res<(String, String)> {
    let me = client::call_json(t, "getMe", &json!({}))?;
    let username = me
        .get("username")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let mut name = me
        .get("first_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if let Some(last) = me.get("last_name").and_then(Value::as_str) {
        if !last.is_empty() {
            name.push(' ');
            name.push_str(last);
        }
    }
    if username.is_empty() && name.is_empty() {
        return Err("توکن ربات درست نیست".to_string());
    }
    Ok((username, name))
}

/// The newest chat that actually sent a message. Updates arrive in Telegram's
/// own order, so the last message wins; anything that is not a message (a
/// membership change, a channel post) is skipped rather than guessed at.
fn newest_chat(updates: &Value) -> Option<(String, Option<String>)> {
    let mut found = None;
    for update in updates.as_array()? {
        if let Some(chat) = chat_of(update) {
            found = Some(chat);
        }
    }
    found
}

fn chat_of(update: &Value) -> Option<(String, Option<String>)> {
    let chat = update.get("message")?.get("chat")?;
    let id = match chat.get("id")? {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        _ => return None,
    };
    let title = chat
        .get("title")
        .and_then(Value::as_str)
        .or_else(|| chat.get("first_name").and_then(Value::as_str))
        .or_else(|| chat.get("username").and_then(Value::as_str))
        .map(|s| s.to_string());
    Some((id, title))
}

fn highest_update_id(updates: &Value) -> Option<i64> {
    updates
        .as_array()?
        .iter()
        .filter_map(|update| update.get("update_id").and_then(Value::as_i64))
        .max()
}

// -------------------------------------------------------------------- the captions

/// The line the customer reads above the PDF: the shop, who it is for, and the
/// total — «جمع کل: ۵٬۱۷۵ تومان».
fn doc_caption(ctx: &Ctx<'_>, doc_id: i64, shop: &str) -> Res<String> {
    let (party, total) = {
        let conn = ctx.conn()?;
        conn.query_row(
            "SELECT p.name, d.total FROM doc d JOIN party p ON p.id = d.party_id WHERE d.id = ?1",
            [doc_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(|_| "فاکتور پیدا نشد".to_string())?
        .ok_or_else(|| "فاکتور پیدا نشد".to_string())?
    };
    let total = format!("جمع کل: {}", crate::money::fmt_toman(total));
    Ok(lines(&[shop, &party, &total]))
}

/// The same for a statement, with the one number that matters: the balance.
fn statement_caption(ctx: &Ctx<'_>, party_id: i64, shop: &str) -> Res<String> {
    let (party, balance) = {
        let conn = ctx.conn()?;
        let party: String = conn
            .query_row("SELECT name FROM party WHERE id = ?1", [party_id], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|_| "طرف حساب پیدا نشد".to_string())?
            .ok_or_else(|| "طرف حساب پیدا نشد".to_string())?;
        let balance = crate::domain::balance::of(&conn, party_id)?;
        (party, balance)
    };
    let title = format!("صورت‌حساب {party}");
    let balance = format!("مانده: {}", crate::money::fmt_toman(balance));
    Ok(lines(&[shop, &title, &balance]))
}

/// Joins the non-empty parts of a caption with newlines. An unset shop name
/// leaves no blank line behind.
fn lines(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

// ------------------------------------------------------------------- the plumbing

/// The token and chat he saved, plus the shop name for the captions. The
/// database lock is released before anything touches the network, so a slow
/// upload never freezes the shop screen.
fn wire(ctx: &Ctx<'_>) -> Res<(String, String, String)> {
    let conn = ctx.conn()?;
    let settings = crate::domain::settings::load(&conn);
    Ok((
        settings.bot_token.trim().to_string(),
        settings.chat_id.trim().to_string(),
        settings.shop_name,
    ))
}

fn saved_chat(ctx: &Ctx<'_>) -> Option<String> {
    let conn = ctx.conn().ok()?;
    let chat = crate::domain::settings::load(&conn).chat_id;
    let chat = chat.trim().to_string();
    if chat.is_empty() {
        None
    } else {
        Some(chat)
    }
}

fn saved_offset(ctx: &Ctx<'_>) -> Option<i64> {
    let conn = ctx.conn().ok()?;
    crate::db::get_setting(&conn, "updates_offset", "")
        .parse::<i64>()
        .ok()
}

/// Saves what onboarding learned. Only these keys are touched, so a token he
/// just pasted can never wipe a shop name or a backup date.
fn save_link(ctx: &Ctx<'_>, token: &str, chat_id: &str, last_update_id: Option<i64>) -> Res<()> {
    let conn = ctx.conn()?;
    crate::db::put_setting(&conn, "bot_token", token.trim())?;
    crate::db::put_setting(&conn, "chat_id", chat_id)?;
    if let Some(last) = last_update_id {
        crate::db::put_setting(&conn, "updates_offset", &(last + 1).to_string())?;
    }
    Ok(())
}

fn token_arg(args: &Value) -> Res<String> {
    let token = args
        .get("token")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if token.is_empty() {
        return Err("توکن ربات را بگذار".to_string());
    }
    Ok(token.to_string())
}

/// Every failure that leaves this module passes through here first.
fn guarded<T>(result: Res<T>, token: &str) -> Res<T> {
    result.map_err(|e| client::scrub(&e, token))
}

/// `{"ok": bool, "message": "…"}` — the shape the invoice screen shows.
fn said(ok: bool, message: &str) -> Value {
    json!({ "ok": ok, "message": message })
}

fn json_of<T: serde::Serialize>(value: T) -> Res<Value> {
    // The one internal failure that is not worth explaining to him.
    serde_json::to_value(value).map_err(|_| "مشکلی پیش آمد".to_string())
}

#[cfg(test)]
pub(crate) const TEST_TOKEN: &str = "123456789:AAH-this-is-not-a-real-token";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    /// A transport that answers from a script and remembers what it was asked.
    struct Stub {
        answers: RefCell<VecDeque<Res<Value>>>,
        asked: RefCell<Vec<String>>,
    }

    impl Stub {
        fn answering(answers: Vec<Res<Value>>) -> Self {
            Self {
                answers: RefCell::new(answers.into_iter().collect()),
                asked: RefCell::new(Vec::new()),
            }
        }
    }

    impl Transport for Stub {
        fn post_json(&self, method: &str, _body: &Value) -> Res<Value> {
            self.asked.borrow_mut().push(method.to_string());
            self.answers
                .borrow_mut()
                .pop_front()
                .unwrap_or_else(|| Ok(json!({ "ok": true, "result": [] })))
        }

        fn post_file(
            &self,
            _method: &str,
            _fields: &[(&str, &str)],
            _file_field: &str,
            _file_name: &str,
            _bytes: &[u8],
        ) -> Res<Value> {
            Ok(json!({ "ok": true, "result": {} }))
        }
    }

    /// A settings-only context: check and link never need a real file.
    fn bench() -> crate::AppState {
        crate::AppState {
            db: std::sync::Mutex::new(db::memory()),
            dirs: crate::Dirs {
                data: std::path::PathBuf::new(),
                files: std::path::PathBuf::new(),
            },
        }
    }

    fn getme() -> Res<Value> {
        Ok(json!({
            "ok": true,
            "result": {
                "id": 123456789,
                "is_bot": true,
                "first_name": "دفتر",
                "last_name": "رضا",
                "username": "daftar_bot"
            }
        }))
    }

    #[test]
    fn check_reads_the_bot_out_of_a_telegram_answer() {
        let state = bench();
        let ctx = Ctx { state: &state };

        // getMe, then getUpdates: he has already pressed Start once.
        let stub = Stub::answering(vec![
            getme(),
            Ok(json!({
                "ok": true,
                "result": [
                    { "update_id": 1, "message": { "message_id": 1, "chat": { "id": 777, "type": "private", "first_name": "رضا" } } }
                ]
            })),
        ]);

        let info = check_with(&ctx, &stub).unwrap();
        assert_eq!(info.username, "daftar_bot");
        assert_eq!(info.name, "دفتر رضا");
        assert_eq!(info.chat_id.as_deref(), Some("777"));
        assert_eq!(info.chat_title.as_deref(), Some("رضا"));
        assert_eq!(
            stub.asked.borrow().as_slice(),
            ["getMe", "getUpdates"],
            "a check asks exactly two questions"
        );

        // Nothing was saved: this screen is still just a check.
        let conn = ctx.conn().unwrap();
        assert_eq!(db::get_setting(&conn, "bot_token", ""), "");
        assert_eq!(db::get_setting(&conn, "chat_id", ""), "");
    }

    #[test]
    fn check_turns_a_telegram_refusal_into_a_persian_sentence() {
        let state = bench();
        let ctx = Ctx { state: &state };

        let stub = Stub::answering(vec![Ok(json!({
            "ok": false,
            "error_code": 401,
            "description": "Unauthorized"
        }))]);

        let message = check_with(&ctx, &stub).unwrap_err();
        assert_eq!(message, "توکن ربات درست نیست");
        assert!(!message.contains(TEST_TOKEN));

        // A dead network comes back as a sentence too, not a panic.
        let offline = Stub::answering(vec![Err("اینترنت وصل نیست".to_string())]);
        assert_eq!(check_with(&ctx, &offline).unwrap_err(), "اینترنت وصل نیست");
    }

    #[test]
    fn link_takes_the_newest_message_chat_and_saves_it() {
        let state = bench();
        let ctx = Ctx { state: &state };

        // Several updates: a Start, a membership change that is not a message,
        // then one unrelated message — the newest message wins.
        let stub = Stub::answering(vec![
            getme(),
            Ok(json!({
                "ok": true,
                "result": [
                    { "update_id": 10, "message": { "message_id": 1, "chat": { "id": 111, "type": "private" } } },
                    { "update_id": 11, "my_chat_member": { "chat": { "id": -100, "type": "group" } } },
                    { "update_id": 12, "message": { "message_id": 2, "chat": { "id": 222, "type": "private", "first_name": "لیلا", "username": "leila" } } }
                ]
            })),
        ]);

        let info = link_with(&ctx, &stub, TEST_TOKEN).unwrap();
        assert_eq!(info.chat_id.as_deref(), Some("222"));
        assert_eq!(info.chat_title.as_deref(), Some("لیلا"));
        assert_eq!(info.username, "daftar_bot");

        // It saved the token and the chat, and remembered where it got to.
        let conn = ctx.conn().unwrap();
        assert_eq!(db::get_setting(&conn, "bot_token", ""), TEST_TOKEN);
        assert_eq!(db::get_setting(&conn, "chat_id", ""), "222");
        assert_eq!(db::get_setting(&conn, "updates_offset", ""), "13");
    }

    #[test]
    fn the_token_never_appears_in_anything_that_is_returned() {
        let state = bench();
        let ctx = Ctx { state: &state };

        // A good answer must not carry the token back to the interface.
        let good = Stub::answering(vec![getme(), Ok(json!({ "ok": true, "result": [] }))]);
        let info = check_with(&ctx, &good).unwrap();
        let as_json = serde_json::to_string(&info).unwrap();
        assert!(!as_json.contains(TEST_TOKEN), "token in BotInfo: {as_json}");

        // A refusal whose description echoes the token is scrubbed to nothing.
        let echo = Stub::answering(vec![Ok(json!({
            "ok": false,
            "error_code": 400,
            "description": format!("bad token {TEST_TOKEN}")
        }))]);
        let message = guarded(check_with(&ctx, &echo), TEST_TOKEN).unwrap_err();
        assert!(!message.contains(TEST_TOKEN), "token in error: {message}");
        assert!(
            message.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c)),
            "not Persian: {message}"
        );
    }
}
