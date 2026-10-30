//! Every operation the interface can ask for, in one place.
//!
//! Each module answers for its own names and returns `None` for the rest, so a
//! new operation is added in one file and nothing else has to change.

pub mod balance;
pub mod cheques;
pub mod home;
pub mod notes;
pub mod orders;
pub mod parties;
pub mod payments;
pub mod products;
pub mod purchases;
pub mod reports;
pub mod search;
pub mod settings;
pub mod sums;
pub mod tasks;

use crate::db::Res;
use crate::Ctx;
use serde_json::Value;

pub fn route(ctx: &Ctx<'_>, op: &str, args: &Value) -> Option<Res<Value>> {
    parties::handle(ctx, op, args)
        .or_else(|| products::handle(ctx, op, args))
        .or_else(|| orders::handle(ctx, op, args))
        .or_else(|| payments::handle(ctx, op, args))
        .or_else(|| purchases::handle(ctx, op, args))
        .or_else(|| cheques::handle(ctx, op, args))
        .or_else(|| tasks::handle(ctx, op, args))
        .or_else(|| notes::handle(ctx, op, args))
        .or_else(|| sums::handle(ctx, op, args))
        .or_else(|| reports::handle(ctx, op, args))
        .or_else(|| search::handle(ctx, op, args))
        .or_else(|| settings::handle(ctx, op, args))
        .or_else(|| home::handle(ctx, op, args))
}
