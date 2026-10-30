//! The Telegram bot: invoices and backups to the owner's own chat.
#![allow(unused_variables)]

use crate::db::Res;
use crate::Ctx;
use serde_json::Value;

pub fn handle(ctx: &Ctx<'_>, op: &str, args: &Value) -> Option<Res<Value>> {
    let _ = (ctx, op, args);
    None
}