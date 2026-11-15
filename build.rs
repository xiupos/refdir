//! For the app: compiles the settings schema next to the build, where the app finds it first, as it
//! has the keys of this build. Without it, the app uses the schema Meson installs

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
