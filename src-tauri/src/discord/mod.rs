//! Discord Rich Presence — muestra en el perfil de Discord del usuario que
//! está en el launcher o jugando, vía el IPC local de Discord (named pipe en
//! Windows, socket Unix en macOS/Linux). No depende de ningún servidor
//! propio: si Discord no está instalado o no está corriendo, simplemente no
//! se conecta y el resto del launcher sigue funcionando normal — nunca debe
//! poder romper nada por su ausencia.

use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use std::sync::Mutex as StdMutex;

pub struct DiscordPresence {
    client_id: StdMutex<Option<String>>,
    client: StdMutex<Option<DiscordIpcClient>>,
}

impl DiscordPresence {
    pub fn new() -> Self {
        Self {
            client_id: StdMutex::new(None),
            client: StdMutex::new(None),
        }
    }

    /// Se llama al cargar la config (o al guardarla desde Configuración). Si
    /// `client_id` es `None` la presencia queda inactiva.
    pub fn configure(&self, client_id: Option<String>) {
        let mut current = self.client_id.lock().unwrap();
        if *current != client_id {
            *current = client_id;
            drop(current);
            // Fuerza reconexión con el nuevo client_id (o desconecta si ahora es None).
            *self.client.lock().unwrap() = None;
        }
    }

    fn ensure_connected(&self) -> bool {
        let mut client_guard = self.client.lock().unwrap();
        if client_guard.is_some() {
            return true;
        }
        let Some(id) = self.client_id.lock().unwrap().clone() else {
            return false;
        };
        let mut candidate = DiscordIpcClient::new(&id);
        match candidate.connect() {
            Ok(()) => {
                *client_guard = Some(candidate);
                true
            }
            Err(err) => {
                tracing::debug!("Discord Rich Presence no disponible: {err}");
                false
            }
        }
    }

    fn set(&self, details: &str, state: &str, started_at: Option<i64>) {
        if !self.ensure_connected() {
            return;
        }
        let mut client_guard = self.client.lock().unwrap();
        let Some(client) = client_guard.as_mut() else { return };

        let mut act = activity::Activity::new().details(details).state(state);
        if let Some(ts) = started_at {
            act = act.timestamps(activity::Timestamps::new().start(ts));
        }
        if client.set_activity(act).is_err() {
            // La conexión se cayó (Discord se cerró, etc.) — se descarta y se
            // reintenta conectar en la próxima actualización de presencia.
            *client_guard = None;
        }
    }

    pub fn set_menu(&self, launcher_name: &str) {
        self.set(launcher_name, "En el menú", None);
    }

    pub fn set_playing(&self, instance_name: &str, minecraft_version: &str, loader_label: &str, started_at_unix: i64) {
        self.set(
            &format!("Jugando {instance_name}"),
            &format!("Minecraft {minecraft_version} · {loader_label}"),
            Some(started_at_unix),
        );
    }
}

impl Default for DiscordPresence {
    fn default() -> Self {
        Self::new()
    }
}
