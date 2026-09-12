fn main() {
    // Release identity comes from Cargo/Tauri package metadata, not a timestamp.
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    println!("cargo:rerun-if-changed=../../ui/dist");
    tauri_build::build();
}
