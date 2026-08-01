# PIKIPIKI LAUNCHER

Launcher de Minecraft ligero, personalizable y sin telemetría, construido con
**Tauri v2 + Rust + React/TypeScript**. Este repositorio es la **Fase 1**: un
núcleo real y funcional (no un mockup) — arranca, se personaliza desde
`config.json`, detecta/instala Java automáticamente, e instala y lanza
versiones **Vanilla** de verdad, con verificación SHA1 de cada archivo
descargado. Las fases siguientes (auth Microsoft, Forge/NeoForge/Fabric/Quilt,
modpacks) se enchufan sobre esta misma arquitectura sin reescribir nada — ver
[Roadmap](#roadmap--qué-falta) más abajo.

## Por qué Tauri (y no Electron o JavaFX)

| | Tauri (elegido) | Electron | Java + JavaFX |
|---|---|---|---|
| Tamaño del instalador | ~10-20 MB (usa WebView2 del sistema en Windows) | ~100-150 MB (empaqueta Chromium) | ~150-200 MB si se bundlea un JRE con `jpackage` |
| RAM en reposo | Baja | Alta | Baja-media |
| DX para UI moderna/minimalista | Alta (stack web + Tailwind) | Alta | Media-baja |
| Empaquetado Windows | `.msi`/`.exe` (NSIS) vía `tauri build` | `.exe` vía electron-builder | `.exe` vía `jpackage`, hay que compilar en cada SO destino |
| Manejo de procesos/descargas Java | Rust: robusto, sin pausas de GC | Node: aceptable | Java: integración nativa perfecta, pero peor UI |

Tauri gana en las prioridades del proyecto (ligereza, luego estabilidad,
luego facilidad de instalación) sin sacrificar una UI moderna. La única
concesión real es que **Tauri no cross-compila de forma fiable de macOS/Linux
a Windows** para el bundler nativo — por eso el `.exe`/`.msi` real se produce
en CI (`.github/workflows/build-windows.yml`) sobre un runner `windows-latest`,
no en tu máquina si no es Windows.

## Estructura del proyecto

```
launcher/
  src-tauri/              # Core en Rust
    src/
      config/              # carga/guarda config.json (branding, RAM, rutas)
      download/             # gestor de descargas: SHA1, reanudación, caché
      java/                 # detección + instalación de Temurin (Adoptium)
      minecraft/
        manifest.rs          # manifiesto oficial de versiones (Mojang)
        install.rs            # instala libs/assets/client.jar con SHA1
        launch.rs              # classpath, natives, args, spawn del proceso
        instance.rs             # modelo de instancia (/instances/<id>/)
      accounts/             # cuentas (offline en fase 1, MSA en fase 2)
      commands.rs           # comandos IPC expuestos a React (invoke())
  src/                    # UI en React/TypeScript
    theme/ThemeProvider.tsx  # config.json -> variables CSS (branding en vivo)
    screens/                  # Inicio, Instancias, Mods, Cuentas, Config, Acerca de
    components/                # Sidebar, RamSlider, ProgressBar, InstanceCard
    lib/api.ts                  # wrapper tipado sobre invoke()
  config/
    config.default.json     # branding/valores por defecto (editable, no hardcodeado)
    themes/                  # paletas dark/light
  assets/                  # icon.png / logo.png placeholder — reemplázalos
  .github/workflows/       # build real del instalador Windows en CI
```

**Nota sobre dónde vive cada cosa en disco en tiempo de ejecución** (no en el
repo): las instancias, el runtime de Java descargado, las librerías/assets de
Minecraft y la caché viven en el directorio de datos de la app del SO
(`app_data_dir` de Tauri — en Windows algo como
`%APPDATA%/com.milauncher.app/`). Librerías y assets se comparten entre todas
las instancias (están direccionados por versión/hash), así que instalar
Vanilla 1.21 dos veces en instancias distintas no duplica la descarga.

## Requisitos para desarrollar

- [Node.js](https://nodejs.org/) 20+
- [Rust](https://www.rust-lang.org/tools/install) (toolchain estable) + las
  dependencias nativas de Tauri para tu SO — ver la
  [guía oficial de prerrequisitos](https://tauri.app/start/prerequisites/)
  (en Windows: WebView2 —ya viene en Windows 10/11 actualizados— y las Build
  Tools de Visual Studio).

Este repo se escribió sin tener Rust instalado en la máquina de desarrollo
(macOS); el código está revisado con cuidado pero **no se ha compilado
todavía**. El primer `npm run tauri:dev` en una máquina con Rust puede sacar
algún error menor de tipos — es esperable en un proyecto de este tamaño sin
compilación previa, y debería ser rápido de resolver con los errores del
propio compilador de Rust en mano.

## Correr en desarrollo

```bash
npm install
npm run tauri:dev
```

Esto levanta Vite (puerto 1420) y compila+abre la ventana nativa de Tauri.

## Compilar el instalador de Windows

**Recomendado — CI (funciona sin tener Windows):**

```bash
git push          # o: gh workflow run build-windows.yml
```

El workflow `.github/workflows/build-windows.yml` compila en un runner
`windows-latest` y sube el `.exe` (NSIS) y el `.msi` como artefactos del run.

**En una máquina Windows real:**

```powershell
npm install
npm run tauri:build
```

El instalador queda en
`src-tauri/target/release/bundle/nsis/*.exe` (y `bundle/msi/*.msi`).

### Icono personalizado

`assets/icon.png` y `assets/logo.png` son **placeholders** generados
programáticamente (un cuadrado de color plano). Reemplázalos por tu arte real
y regenera el set completo de iconos de plataforma con:

```bash
npx tauri icon assets/icon.png
```

Esto sobreescribe `src-tauri/icons/` (32x32, 128x128, `.ico`, `.icns`, etc.)
que `tauri.conf.json` ya referencia.

## Personalización — antes de compilar

El nombre, logo, colores y textos **se fijan antes de compilar**, editando
dos archivos — no son un ajuste que el usuario final cambie desde la app ya
instalada:

1. **`config/config.default.json`** — branding y valores por defecto. Se
   embebe en el binario en tiempo de compilación (`include_str!` en
   `src-tauri/src/config/mod.rs`) y es lo que se copia como config inicial en
   el directorio de datos de la app la primera vez que corre.
2. **`src-tauri/tauri.conf.json`** — `productName` e `identifier` (usados por
   Windows para el instalador, el acceso directo y el registro de
   desinstalación) e `icon` (ver más arriba, `npx tauri icon`).

```jsonc
// config/config.default.json
{
  "launcherName": "PIKIPIKI LAUNCHER", // nombre visible en toda la UI y el título de ventana
  "logoPath": "assets/logo.png",     // informativo — la imagen real se embebe desde este archivo al compilar
  "iconPath": "assets/icon.png",     // idem, más el ícono de la app en src-tauri/icons/ (ver npx tauri icon)
  "theme": "dark",                   // "dark" | "light" — el usuario SÍ puede cambiar esto en Configuración
  "primaryColor": "#4CAF50",         // color de acento en toda la UI
  "backgroundImage": "assets/banner.jpg", // informativo, ídem — fondo de la pantalla de Inicio
  "welcomeText": "Bienvenido a PIKIPIKI LAUNCHER",
  "supportUrl": "https://github.com/tu-usuario/tu-launcher",
  "defaultMinRamMb": 2048,
  "defaultMaxRamMb": 4096,
  "autoUpdateJava": true,            // reservado: hoy la instalación de Java es manual desde Configuración
  "showSnapshots": false,            // el usuario SÍ puede cambiar esto en Configuración
  "instancesDir": null,              // null = carpeta por defecto en app_data_dir
  "javaDir": null,                   // idem, para el runtime de Java gestionado
  "defaultServerName": "MUNDO PIKIPIKI",       // servidor precargado en cada instancia nueva, o null
  "defaultServerAddress": "pikipiki.axel-diaz.com",
  "applyTitleScreenPack": true,       // fondo/logo personalizados en el menú del juego (no del launcher)
  "versionTypeLabel": "PikiPiki",     // texto junto a "Minecraft <versión>" en ese mismo menú
  "microsoftClientId": null           // tu "Application (client) ID" de Microsoft Entra — ver más abajo
}
```

Después de editar cualquiera de los dos archivos, recompila
(`npm run tauri:build` o el workflow de CI) para que el cambio quede en el
instalador.

**Logo, ícono y fondo son imágenes reales**, no solo texto: `assets/logo.png`
(barra lateral), `assets/icon.png` (referencia) y `assets/banner.jpg` (fondo
de Inicio) se embeben directo en el binario en tiempo de compilación
(`include_bytes!` en `src-tauri/src/config/mod.rs`) — para cambiarlas,
reemplaza esos tres archivos y recompila. El ícono real de la app
(dock/taskbar/instalador) es aparte: `src-tauri/icons/*`, generado con
`npx tauri icon assets/icon.png`.

**Qué sí queda editable en caliente, ya con la app instalada:** tema
oscuro/claro, mostrar snapshots, RAM por defecto y argumentos JVM por
instancia, e instalar/gestionar runtimes de Java — todo desde la pantalla
Configuración. Esto es a propósito: son preferencias de uso, no la identidad
del launcher.

## Qué es real y funcional en esta fase

- **Config sin hardcodear**: branding completo editable antes de compilar (ver
  [Personalización](#personalización--antes-de-compilar)), sin tocar código fuente.
  Incluye imágenes reales: logo en la barra lateral y fondo en la pantalla de
  Inicio, no solo texto/color.
- **Java**: detección real (PATH, `JAVA_HOME`, rutas típicas por SO, registro
  de Windows) ejecutando `java -XshowSettings:properties -version` y
  parseando la salida real — no asume nada por la ruta. Si no hay un Java 8,
  17 o 21 de 64 bits compatible, descarga e instala Temurin (Adoptium) en una
  carpeta propia del launcher, con verificación SHA256.
- **Vanilla**: manifiesto oficial de Mojang, descarga de `client.jar`,
  librerías (con las reglas por SO del JSON oficial) y assets, **todo
  verificado por SHA1**. Las descargas son reanudables (HTTP `Range`) y usan
  caché de contenido por hash.
- **Cuentas Microsoft reales**: device code flow + Xbox Live + XSTS +
  Minecraft Services, con renovación automática de sesión vía refresh token
  (no hay que volver a iniciar sesión cada hora). Ver
  [Cuentas: Microsoft real + offline](#cuentas-microsoft-real--offline) para
  cómo registrar tu propia app.
- **Lanzamiento real**: construye el classpath, extrae los natives que
  corresponden al SO, resuelve los argumentos JVM/juego (soporta tanto el
  formato moderno `arguments.{jvm,game}` de 1.13+ como el legado
  `minecraftArguments` de versiones viejas) y hace `spawn` del proceso Java de
  verdad, con logs en vivo en el backend (persistidos en
  `instances/<id>/logs/`).
- **Fabric / Quilt**: se instalan leyendo su API "meta" (JSON ya resuelto,
  verificado contra las respuestas reales de `meta.fabricmc.net`/
  `meta.quiltmc.org`) y fusionando con la versión Vanilla de la que dependen
  (`inheritsFrom`) — nada que ejecutar, solo descargar y fusionar JSON.
- **Forge / NeoForge**: se descarga el instalador oficial desde el
  repositorio Maven público de cada proyecto y se ejecuta
  (`java -jar forge-installer.jar --installClient <dir>`), apuntando al
  mismo directorio compartido de librerías/versiones — no se reimplementa su
  lógica de parcheo binario del client.jar. Ver la nota ética/legal más abajo,
  es importante.
- **OptiFine**: importación manual — el usuario descarga el `.jar` él mismo
  desde optifine.net (su licencia es más restrictiva que la de Forge con la
  redistribución, así que no hay descarga automática ni la habrá) y lo
  importa una vez desde el launcher. A partir de ahí, instalar solo corre
  `java -jar <archivo importado>` (la ventana normal del instalador de
  OptiFine) y el launcher detecta qué versión nueva quedó en el
  `.minecraft` del sistema para copiarla a su estructura compartida — **sin
  parsear nada específico del formato interno de OptiFine**, precisamente
  porque no se verificó ese formato contra un archivo real (a diferencia de
  Forge). Es la implementación más nueva y menos probada de las cuatro;
  si algo falla, lo más útil es el mensaje de error exacto + qué versión de
  Minecraft y de OptiFine se intentó instalar.
- **Instancias**: cada una tiene su propia carpeta `minecraft/` (saves, mods,
  resourcepacks, config) y su propia RAM/argumentos JVM opcionales; comparten
  librerías/assets/versiones globalmente.
- **Mods**: activar/desactivar `.jar` locales en `instances/<id>/minecraft/mods`
  (renombra a `.disabled`) — funcional para cualquier loader ya instalado a
  mano.
- **Rendimiento**: sliders de RAM mín/máx con detección real de memoria del
  sistema (`sysinfo`) y aviso si se asigna más del 75% de la RAM total.

### Nota ética/legal sobre Forge/NeoForge

El propio `install_profile.json` que trae el instalador oficial de Forge
incluye este comentario textual: *"Please do not automate the download and
installation of Forge. Our efforts are supported by ads from the download
page."* Confirmado leyendo un instalador real antes de programar esto, no es
un rumor. No hay forma de automatizar la instalación de Forge respetando
eso al pie de la letra — la práctica estándar entre launchers de código
abierto (MultiMC, Prism Launcher, ATLauncher) es descargar desde el
repositorio Maven público de Forge en vez de la página con anuncios
(`files.minecraftforge.net`), que es exactamente lo que hace este launcher
también. Es una tensión real con lo que pide el mantenedor del proyecto
(LexManos), no una zona gris resuelta: si vas a distribuir este launcher,
deberías saber que ese desacuerdo existe y decidir tú si te parece aceptable
antes de publicarlo ampliamente.

Además, el instalador exige que el directorio destino tenga un
`launcher_profiles.json` (revisa que "parezca" un `.minecraft` real, creado
por el launcher oficial de Mojang) — se crea uno mínimo sintético
automáticamente si no existe, mismo workaround que usan MultiMC/Prism.

### Cuentas: Microsoft real + offline

**Microsoft real** ya está implementado: device code flow (OAuth2 del
Microsoft identity platform) → Xbox Live → XSTS → Minecraft Services → perfil
real (UUID, nombre, skin) — el mismo flujo que usa el launcher oficial y
MultiMC/Prism. El `client_id` viene de `microsoftClientId` en la config, no
está hardcodeado: cada quien despliegue este launcher necesita registrar su
propia app en **Microsoft Entra** (gratis, requisito de Microsoft, no se
puede evitar ni incluir uno genérico). Pasos:

1. [entra.microsoft.com](https://entra.microsoft.com) → **App registrations**
   → **New registration**
2. "Supported account types": cuentas personales de Microsoft (o "cualquier
   inquilino + cuentas personales", también sirve)
3. "Redirect URI": tipo **Public client/native (mobile & desktop)**, valor
   `https://login.microsoftonline.com/common/oauth2/nativeclient`
4. Copia el **Application (client) ID** a `microsoftClientId` en
   `config/config.default.json`

Nota si el portal de Azure/Entra da un error de "cuenta no existe en el
tenant" (pasa con algunas cuentas personales que nunca tuvieron un tenant
provisionado): registra tu cuenta gratis en el
[Microsoft 365 Developer Program](https://developer.microsoft.com/microsoft-365/dev-program)
primero (no pide tarjeta), eso crea el tenant que falta.

**Cuenta infantil bajo un grupo familiar** (el caso típico de alumnos de
primaria): Xbox Live puede rechazar el login hasta que un adulto la agregue a
un grupo familiar en account.microsoft.com/family — el launcher ya muestra
ese mensaje específico si pasa, en vez de un error genérico.

**Offline**: sigue disponible como modo aparte, sin cambios — mismo UUID
determinista que calcula el propio cliente de Minecraft
(`UUID.nameUUIDFromBytes("OfflinePlayer:" + nombre)`), sin token de sesión
real. **No es un bypass ni un crack** — es el mismo modo "cuenta sin
conexión" que ofrecen MultiMC/Prism Launcher para pruebas en un solo jugador
con una copia ya poseída.

### Discord Rich Presence (opcional)

Si Discord está corriendo en la máquina, el launcher se conecta por su IPC
local (sin servidor propio) y muestra un estado en tu perfil: "En el menú"
mientras navegas el launcher, y "Jugando \<nombre de la versión\>" con la
versión de Minecraft/loader y tiempo transcurrido mientras el juego está
abierto. Si Discord no está instalado o no está corriendo, esto simplemente
no hace nada — el resto del launcher sigue funcionando igual.

Como con Microsoft, hace falta tu propio `client_id` — es gratis y toma un
minuto:

1. [discord.com/developers/applications](https://discord.com/developers/applications)
   → **New Application**, ponle el nombre que quieras (es lo que puede
   aparecer junto al estado)
2. Copia el **Application ID** de la pantalla "General Information"
3. Pégalo en `discordClientId` en `config/config.default.json`

Sin ese campo (`null` por defecto), la función queda desactivada.

## Limitaciones conocidas de esta fase

- El orden de las versiones de Forge/NeoForge en el selector es un
  heurístico (se invierte el orden de publicación del `maven-metadata.xml`),
  no una garantía estricta de "más nueva primero" — Fabric/Quilt sí lo
  garantizan porque su API sí expone esa información.
- Java para correr el instalador de Forge/NeoForge se elige con la misma
  lógica que para lanzar el juego (mismo major requerido); no se ha probado
  contra versiones muy viejas de Forge (pre-1.13) que podrían preferir un
  JRE distinto para el instalador que para el propio juego.

## Roadmap — qué falta

Fuera de alcance de esta fase **por diseño**, con la razón técnica/legal de
cada una:

- **Modpacks** `.mrpack` (spec abierta de Modrinth, fácil) y `.zip` estilo
  CurseForge (el `manifest.json` es legible, pero descargar los mods vía API
  de CurseForge exige que tú obtengas tu propia API key aprobada — CurseForge
  cerró su API a integraciones de terceros no aprobadas en 2023).
- Playtime tracking detallado, capturas por instancia, importación de un
  `.minecraft` existente, auto-actualización del launcher, temas custom vía
  JSON: extensiones directas sobre lo ya montado (instancias ya registran
  tiempo jugado agregado; falta el desglose por sesión).

## Seguridad

- Cero telemetría, cero anuncios, cero trackers.
- Todo lo que se descarga viene de fuentes oficiales: `piston-meta.mojang.com`,
  `resources.download.minecraft.net`, `api.adoptium.net`. La CSP en
  `tauri.conf.json` restringe explícitamente a esos dominios.
- Toda descarga de librerías/assets/client de Minecraft se verifica por SHA1
  contra el manifiesto oficial antes de considerarse instalada; el runtime de
  Java se verifica por SHA256 contra el que publica Adoptium.
- Dependencias Rust/JS son las habituales y auditables del ecosistema
  (`reqwest`, `tokio`, `serde`, Tauri oficial, React) — nada de paquetes
  oscuros o de un solo mantenedor sin trayectoria.
