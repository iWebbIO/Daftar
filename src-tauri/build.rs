fn main() {
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
