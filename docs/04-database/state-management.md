# 🗄️ Gestión del Estado y Persistencia en Memoria — NxDesk

| Código | DAT-STA-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Core Architecture Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Filosofía de Persistencia "In-Memory First"

NxDesk no utiliza bases de datos relacionales pesadas (como PostgreSQL o SQL Server) en el cliente local. Dado que el software está diseñado para operar con ultra-baja latencia y máxima privacidad, el estado de sesión, las identidades de pares descubiertos y los canales de streaming residen **estrictamente en memoria volátil (RAM)**.

---

## 2. Primitivas de Concurrencia y Sincronización en Rust

El estado global del cliente (`AppState`) y del servidor (`HubState`) se coordina mediante primitivas de alta concurrencia diseñadas para evitar condiciones de carrera (Race Conditions) y deadlocks:

* **`Arc<RwLock<T>>`:** Utilizado para estructuras con lecturas muy frecuentes y escrituras esporádicas:
  * `active_session`: Comprobada en cada evento de entrada; modificada solo al conectar o desconectar.
  * `discovered_peers`: Consultada en cada frame de la UI; actualizada cuando el radar UDP recibe un ping.
  * `latest_jpeg_frame`: Actualizada a 30-60 FPS por el capturador y leída por el widget de visualización.
* **`Arc<Mutex<T>>`:** Utilizado para recursos con exclusión mutua estricta:
  * `input_injector`: Previene que dos eventos concurrentes de Win32 interfieran en la misma llamada de API.
  * `capturer`: Garantiza que el búfer GDI / DXGI sea leído por un solo hilo a la vez.
  * `session_waiters`: Mapa de canales `oneshot::Sender` para responder a solicitudes pendientes.
* **`Arc<AtomicBool>` / `Arc<AtomicUsize>`:** Flags atómicos de acceso ultrarrápido sin locks:
  * `is_signaling_connected`: Indica si el WebSocket de señalización está en línea.
  * `room_occupancy`: Número de participantes en la sala actual.
  * `connected_viewers`: Contador de clientes conectados.

---

## 3. Limitador de Tasa en Memoria (`InMemoryRateLimiter`)

Para mitigar ataques de fuerza bruta o saturación de peticiones de conexión:
```rust
pub struct InMemoryRateLimiter {
    attempts: Arc<Mutex<HashMap<IpAddr, (u32, Instant)>>>,
}
```
* Cada dirección IP dispone de una ventana de tiempo deslizante (ej. 60 segundos) con un número máximo de intentos autorizados (ej. 5 intentos de contraseña).
* Se aplica una rutina de poda automática cuando el mapa supera los 1,000 registros para evitar fugas de memoria por agotamiento de descriptores.
