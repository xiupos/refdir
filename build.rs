//! For the app: compiles the settings schema next to the build, so that the app finds it when it
//! is not installed (`cargo run`). Installed, it uses the schema Meson installs

fn main() {
    let out = std::env::var("OUT_DIR").unwrap();
    println!("cargo:rustc-env=SCHEMA_DIR={out}");
    println!("cargo:rerun-if-changed=data/net.xiupos.Refdir.gschema.xml");
    if std::env::var_os("CARGO_FEATURE_GUI").is_none() {
        return;
    }
    let status = std::process::Command::new("glib-compile-schemas")
        .args(["--strict", "--targetdir", &out, "data"])
        .status()
        .expect("glib-compile-schemas, from GLib, is needed to build the app");
    assert!(status.success(), "the settings schema does not compile");
}
