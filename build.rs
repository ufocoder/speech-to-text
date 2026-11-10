fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let dir = std::env::var("CARGO_MANIFEST_DIR").expect("project directory");
        println!("cargo:rerun-if-changed=Info.plist");
        for argument in [
            "-sectcreate",
            "__TEXT",
            "__info_plist",
            &format!("{dir}/Info.plist"),
        ] {
            println!("cargo:rustc-link-arg-bin=speech-to-text=-Xlinker");
            println!("cargo:rustc-link-arg-bin=speech-to-text={argument}");
        }
    }
}
