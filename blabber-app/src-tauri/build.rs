fn main() {
    println!("cargo:rerun-if-changed=assets/sounds");
    tauri_build::build()
}
