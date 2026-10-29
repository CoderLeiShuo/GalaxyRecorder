fn main() {
    // Tauri supplies Rust flags for Android, so emit these at the final link step.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        println!("cargo:rustc-link-arg=-Wl,-z,max-page-size=16384");
        println!("cargo:rustc-link-arg=-Wl,-z,common-page-size=16384");
    }
    tauri_build::build();
    #[cfg(target_os = "macos")]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios") {
        let api = std::path::PathBuf::from(
            std::env::var("DEP_TAURI_IOS_LIBRARY_PATH").expect("missing Tauri iOS API"),
        );
        let output = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
        let package = output.join("RecorderIOS");
        copy_sources(&api, &output.join(".tauri/tauri-api"));
        copy_sources(std::path::Path::new("ios"), &package);
        println!("cargo:rerun-if-changed=ios");
        tauri_utils::build::link_apple_library("RecorderIOS", package);
    }
}

#[cfg(target_os = "macos")]
fn copy_sources(source: &std::path::Path, target: &std::path::Path) {
    std::fs::create_dir_all(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if [".build", ".swiftpm", "Tests", "Package.resolved"]
            .contains(&name.to_str().unwrap_or(""))
        {
            continue;
        }
        let path = target.join(name);
        if entry.file_type().unwrap().is_dir() {
            copy_sources(&entry.path(), &path);
        } else {
            std::fs::copy(entry.path(), path).unwrap();
        }
    }
}
