fn main() {
    println!("cargo:rerun-if-env-changed=ANTHRO_BRIDGE_CHANNEL");
    tauri_build::build()
}
