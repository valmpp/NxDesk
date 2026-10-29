# 🖱️ Módulo: Inyección de Entrada (Teclado y Ratón) — NxDesk

| Código | MOD-INP-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Low-Level Systems Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Propósito y Responsabilidades
El módulo de **Inyección de Entrada** (`crates/input` en Rust y `WindowsInputInjectionService.cs` / `LinuxInputInjectionService.cs` en C#) se encarga de recibir los eventos de usuario generados en el cliente remoto y reproducirlos fielmente en el sistema operativo del equipo anfitrión.

---

## 2. Eventos Soportados (`InputEvent`)

```rust
pub enum InputEvent {
    MouseMove { x: f32, y: f32 },
    MouseDown { button: MouseButton },
    MouseUp { button: MouseButton },
    MouseWheel { delta_y: f32 },
    KeyDown { key_code: u32 },
    KeyUp { key_code: u32 },
    Text { text: String },
}
```

* **Coordenadas Normalizadas (0.0 a 1.0):**  
  Las posiciones del ratón se transmiten como porcentajes normalizados respecto a las dimensiones de la pantalla remota. El inyector en el anfitrión escala estas coordenadas al ancho y alto físico del monitor seleccionado:
  $$\text{Pixel}_X = \text{Bounds}_X + \lfloor x \times \text{Width} \rfloor$$
  $$\text{Pixel}_Y = \text{Bounds}_Y + \lfloor y \times \text{Height} \rfloor$$

---

## 3. Implementación con Win32 API

En Windows, la síntesis se ejecuta mediante la API nativa `SendInput`:
* **Movimiento Absoluto:** Utiliza banderas `MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE` mapeadas al espacio virtual de 65535x65535.
* **Clics y Rueda:** `MOUSEEVENTF_LEFTDOWN`, `MOUSEEVENTF_LEFTUP`, `MOUSEEVENTF_WHEEL`.
* **Teclado:** `INPUT_KEYBOARD` con `KEYEVENTF_SCANCODE` y `KEYEVENTF_KEYUP` para garantizar compatibilidad con videojuegos o terminales de consola.

---

## 4. Políticas de Seguridad
Para consultar la validación de sesiones y barreras de protección de entrada:  
👉 **[security.md](file:///c:/Users/User/Downloads/nxDesk/NxDesk/NxDesk/docs/02-modules/input-injection/security.md)**
