use std::fs;

fn main() {
    // The app version lives in tauri.conf.json and is compiled in as
    // DAFTAR_VERSION. Reading it here rather than from Cargo.toml matters: the
    // updater compares this number against the release tag, and a crate version
    // that changed on every release would rewrite Cargo.lock and throw away the
    // CI cache each time.
    if let Ok(conf) = fs::read_to_string("tauri.conf.json") {
        let version = conf
            .split("\"version\"")
            .nth(1)
            .and_then(|rest| rest.split('"').nth(1))
            .unwrap_or("0.0.0");
        println!("cargo:rustc-env=DAFTAR_VERSION={version}");
    }
    println!("cargo:rerun-if-changed=tauri.conf.json");

    // The share plugin is written in Kotlin and registered from Rust, so its
    // permissions are generated here rather than coming from a crate. Without
    // this the capability in capabilities/default.json has nothing to grant and
    // the interface's `plugin:share|share` call is rejected by the ACL.
    tauri_build::try_build(
        tauri_build::Attributes::new().plugin(
            "share",
            tauri_build::InlinedPlugin::new()
                .commands(&["share", "shareFile", "installApk"])
                .default_permission(tauri_build::DefaultPermissionRule::AllowAllCommands),
        ),
    )
    .expect("tauri-build failed");
}
