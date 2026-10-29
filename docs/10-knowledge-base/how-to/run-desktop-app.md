# 💡 Guía Rápida: Cómo Ejecutar la Aplicación de Escritorio — NxDesk

| Código | KB-HOW-001 | Versión | v1.0 |
|--------|------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Desktop Engineering Team |
| Estado | Aprobado | Clasificación | Base de Conocimiento (KB) |

---

## Cuándo Usar Esta Guía
Utiliza esta guía para arrancar el entorno de desarrollo y probar la conexión de escritorio remoto entre dos instancias locales o en red.

---

## Diagnóstico Previo
Verificar que `cargo` esté disponible:
```powershell
cargo --version
```

---

## Solución Paso a Paso

### 1. Iniciar el Servidor de Señalización
En una terminal:
```powershell
cd nxdesk-rust
cargo run -p nxdesk-server
```
*Verás el banner oficial: `NxDesk Signaling Server (Rust Native) Escuchando en: http://0.0.0.0:5000/signalinghub`.*

### 2. Iniciar el Cliente Anfitrión (Host)
En una segunda terminal:
```powershell
cd nxdesk-rust
cargo run -p nxdesk-client -- --alias="Host-Principal" --device-id="111-222-333"
```
*Se abrirá la ventana principal de escritorio con el ID `111-222-333`.*

### 3. Iniciar el Cliente Operador (Viewer)
En una tercera terminal:
```powershell
cd nxdesk-rust
cargo run -p nxdesk-client -- --alias="Operador-Tecnico" --device-id="999-888-777"
```
*Digita `111-222-333` en la barra superior del Operador y presiona `Enter`. El Host verá el diálogo modal para aceptar la conexión.*

---

## Resultado Esperado
* El Operador entra a la vista `RemoteView` con el stream en vivo a 30 FPS.
* El Host visualiza el widget flotante con el botón rojo `Desconectar`.

---

## Qué NO Hacer
* ❌ **NO intentar abrir `http://localhost:8080` en un navegador web.** El servidor web Axum fue eliminado permanentemente; NxDesk opera de forma nativa en su propia ventana de escritorio.
