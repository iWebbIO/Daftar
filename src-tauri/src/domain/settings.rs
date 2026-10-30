//! Settings, and the one call the interface makes when it starts.

use crate::db::{self, Res};
use crate::dto::Settings;
use crate::Ctx;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;

pub const KEYS: [(&str, &str); 13] = [
    ("shop_name", ""),
    ("shop_phone", ""),
    ("shop_address", ""),
    ("bank_name", ""),
    ("card_holder", ""),
    ("card_number", ""),
    ("invoice_note", ""),
    ("round_step", "0"),
    ("bot_token", ""),
    ("chat_id", ""),
    ("currency", "toman"),
    ("backup_hour", "21"),
    ("theme", "light"),
];

pub fn load(conn: &Connection) -> Settings {
    let get = |k: &str, d: &str| db::get_setting(conn, k, d);
    Settings {
        shop_name: get("shop_name", ""),
        shop_phone: get("shop_phone", ""),
        shop_address: get("shop_address", ""),
        bank_name: get("bank_name", ""),
        card_holder: get("card_holder", ""),
        card_number: get("card_number", ""),
        invoice_note: get("invoice_note", ""),
        round_step: get("round_step", "0").parse().unwrap_or(0),
        bot_token: get("bot_token", ""),
        chat_id: get("chat_id", ""),
        currency: get("currency", "toman"),
        backup_hour: get("backup_hour", "21").parse().unwrap_or(21),
        last_backup_at: get("last_backup_at", "").parse().ok(),
        theme: get("theme", "light"),
    }
}

pub fn save(conn: &Connection, s: &Settings) -> Res<()> {
    db::put_setting(conn, "shop_name", &s.shop_name)?;
    db::put_setting(conn, "shop_phone", &s.shop_phone)?;
    db::put_setting(conn, "shop_address", &s.shop_address)?;
    db::put_setting(conn, "bank_name", &s.bank_name)?;
    db::put_setting(conn, "card_holder", &s.card_holder)?;
    db::put_setting(conn, "card_number", &s.card_number)?;
    db::put_setting(conn, "invoice_note", &s.invoice_note)?;
    db::put_setting(conn, "round_step", &s.round_step.to_string())?;
    db::put_setting(conn, "bot_token", &s.bot_token)?;
    db::put_setting(conn, "chat_id", &s.chat_id)?;
    db::put_setting(conn, "currency", &s.currency)?;
    db::put_setting(conn, "backup_hour", &s.backup_hour.to_string())?;
    db::put_setting(conn, "theme", &s.theme)?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveArgs {
    settings: Settings,
}

pub fn handle(ctx: &Ctx<'_>, op: &str, args: &Value) -> Option<Res<Value>> {
    Some(match op {
        "settings.get" => (|| {
            let conn = ctx.conn()?;
            Ok(serde_json::to_value(load(&conn)).map_err(|e| e.to_string())?)
        })(),
        "settings.save" => (|| {
            let a: SaveArgs = serde_json::from_value(args.clone())
                .map_err(|e| format!("تنظیم‌ها خوانده نشد: {e}"))?;
            let conn = ctx.conn()?;
            save(&conn, &a.settings)?;
            Ok(Value::Bool(true))
        })(),
        _ => return None,
    })
}
