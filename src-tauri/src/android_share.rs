//! The phone's own share sheet — the Rust half.
//!
//! The shopkeeper asked for both ways out of the app: his Telegram bot, which
//! this crate uploads to itself, and the normal Android share sheet, where he
//! picks WhatsApp or the SMS app or a printer. Neither replaces the other; the
//! screen offers both and he chooses at the moment of sending.
//!
//! Almost all of this plugin is Kotlin, in
//! `gen/android/app/src/main/java/io/moreweb/daftar/SharePlugin.kt`. Tauri only
//! needs to be told the plugin exists, because a command that starts with
//! `plugin:` is routed to the Android side when no Rust handler answers it. So
//! the work here is one registration:
//!
//! ```text
//! invoke("plugin:share|share", { text })        → text/plain
//! invoke("plugin:share|shareFile", { path, mime, text }) → one file
//! ```
//!
//! Two things outside this file are needed for that invoke to arrive:
//!
//! 1. the plugin is declared as an inlined plugin in `build.rs`, which is what
//!    generates its permission set, and
//! 2. `capabilities/default.json` grants `share:default`.
//!
//! Without the capability Tauri's access control drops the call before it ever
//! reaches Kotlin — with a message about the ACL, which is not something a
//! shopkeeper should ever have to read.
//!
//! Every other platform registers the plugin and nothing more: there is no
//! share sheet to open on Windows or Linux, and this crate is built there too.

use tauri::plugin::{Builder, TauriPlugin};
use tauri::Runtime;

/// The Android package of the Kotlin class, dotted the way `Class.forName`
/// wants it. Tauri turns it into `io/moreweb/daftar` and appends the class.
#[cfg(target_os = "android")]
const PLUGIN_IDENTIFIER: &str = "io.moreweb.daftar";

/// The Kotlin class Tauri instantiates at startup. It must have a constructor
/// taking the activity and nothing else.
#[cfg(target_os = "android")]
const PLUGIN_CLASS: &str = "SharePlugin";

/// The name in `plugin:share|…`. The other end of it is `Builder::new` below.
pub const NAME: &str = "share";

/// Registers the share plugin. Add it to the builder with `.plugin(...)`.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    #[cfg(target_os = "android")]
    {
        Builder::new(NAME)
            .setup(|_app, api| {
                // This instantiates `io.moreweb.daftar.SharePlugin` with the
                // current activity and files it under `NAME`. Every command it
                // answers lives in Kotlin, so there is no Rust command here to
                // register — Tauri forwards unknown plugin commands to Android
                // by itself.
                let _handle = api.register_android_plugin(PLUGIN_IDENTIFIER, PLUGIN_CLASS)?;
                Ok(())
            })
            .build()
    }

    // Nowhere else is there a share sheet to open, and this crate is built for
    // Windows in CI. The name is still registered so the same interface code
    // compiles everywhere; the commands simply are not there to call.
    #[cfg(not(target_os = "android"))]
    {
        Builder::new(NAME).build()
    }
}
