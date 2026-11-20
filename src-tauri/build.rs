fn main() {
    // Recompila (y vuelve a incrustar) si cambia el frontend estático.
    println!("cargo:rerun-if-changed=../ui");
    tauri_build::build()
}
