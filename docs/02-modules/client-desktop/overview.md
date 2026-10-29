# 🖥️ Módulo: Cliente de Escritorio Nativo — NxDesk

| Código | MOD-CLI-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Desktop Engineering Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Propósito y Responsabilidades
El módulo **Cliente de Escritorio** es el punto de interacción visual principal del usuario final. En el ecosistema Rust reside en `nxdesk-rust/apps/client` y en C# en `src/NxDesk.Client`.  
Es responsable de:
1. Renderizar la interfaz visual con identidad AnyDesk Dark (`#1E1E1E` y `#007ACC`).
2. Conectar y autenticar contra el servidor de señalización.
3. Emitir y recibir balizas de descubrimiento LAN por UDP 5001.
4. Gestionar el ciclo de vida de la sesión (petición, negociación de permisos, desconexión).
5. Pintar la pantalla remota en un canvas acelerado por GPU y capturar eventos de teclado/ratón.

---

## 2. Estructura Interna del Cliente Rust (`nxdesk-client`)

* **`main.rs`:** Punto de entrada del binario nativo. Parsea argumentos de línea de comandos, inicializa la configuración, arranca las tareas en segundo plano de Tokio (radar UDP, cliente de señalización, bucle de eventos) e inicia la ventana `eframe` con backend `Glow`.
* **`app.rs`:** Bucle de interfaz de usuario inmediata (`eframe::App`). Dibuja la vista principal (`HomeView`), la vista remota (`RemoteView`), la tarjeta de sesión activa y los diálogos modales de aceptación.
* **`state.rs`:** Estado global compartido `AppState`. Protegido por `Arc<RwLock>` y primitivas atómicas.
* **`auth.rs`:** Gestor de autenticación de contraseñas de anfitrión y tokens efímeros.
* **`capture_pipeline.rs`:** Hilo desacoplado de captura Win32 GDI y compresión JPEG.

---

## 3. Paleta de Colores Oficial (Avalonia Dark Theme Parity)

| Token de Color | Valor Hex | Uso en la Interfaz |
| :--- | :---: | :--- |
| **Fondo Principal** | `#1E1E1E` | Fondo de la ventana principal y paneles base. |
| **Fondo Secundario** | `#252526` | Encabezado (Header), barra de búsqueda y contenedor de puesto de trabajo. |
| **Superficie de Tarjetas** | `#2A2A2A` | Tarjetas de equipos descubiertos (`DeviceCard`) y modales. |
| **Bordes y Divisores** | `#333333` | Líneas de división y bordes de tarjetas. |
| **Primario / Énfasis** | `#007ACC` | Logo NxDesk, texto grande del ID AnyDesk y botones de acción principal. |
| **Estado En Línea** | `#00CC66` | Punto indicador de disponibilidad de pares en red. |
| **Peligro / Desconectar** | `#D13438` | Botón de finalización de sesión y advertencias de seguridad. |
