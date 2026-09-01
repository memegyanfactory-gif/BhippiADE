//! Exports typed IPC bindings to `ui/src/lib/ipc.ts` without opening a window.
//! Run: `cargo run -p bhippi-app --bin export-bindings` (INV-032 regeneration).

fn main() {
    match bhippi_app::export_bindings() {
        Ok(path) => println!("bindings written to {}", path.display()),
        Err(error) => {
            eprintln!("failed to export IPC bindings: {error}");
            std::process::exit(1);
        }
    }
}
