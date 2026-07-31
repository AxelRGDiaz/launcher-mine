// Punto de entrada fino: toda la lógica vive en `lib.rs` (patrón estándar de
// Tauri v2, necesario para poder reusar el mismo core en builds móviles).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    launcher_lib::run();
}
