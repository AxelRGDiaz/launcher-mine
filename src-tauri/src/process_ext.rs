//! En Windows, lanzar un proceso sin esto abre una ventana de consola negra
//! detrás de la GUI (aunque sea una fracción de segundo) — ni `java
//! -version` para detectar instalaciones, ni los instaladores silenciosos de
//! Forge/OptiFine, ni el propio Minecraft al jugar necesitan una consola.

#[cfg(windows)]
pub fn hide_console(cmd: &mut tokio::process::Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub fn hide_console(_cmd: &mut tokio::process::Command) {}
