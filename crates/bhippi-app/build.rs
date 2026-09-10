use chrono::Local;

fn main() {
    // The shipped version carries the minute the binary was built —
    // `1.1.MMDDYYYYHHMM` — so any build handed to someone is identifiable on
    // sight. The crate's own `version` stays valid semver (a leading zero in a
    // numeric identifier is not, and Cargo would refuse it), and this stamp is
    // what the app reports and shows.
    let stamp = Local::now().format("%m%d%Y%H%M").to_string();
    println!("cargo:rustc-env=BHIPPI_BUILD_VERSION=1.1.{stamp}");
    println!("cargo:rerun-if-changed=../../ui/dist");
    tauri_build::build();
}
