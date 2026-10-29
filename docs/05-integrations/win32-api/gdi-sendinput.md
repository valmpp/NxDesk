# 🪟 Integración: Win32 API (GDI y SendInput) — NxDesk

| Código | INT-W32-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Windows Low-Level Engineering |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Captura de Pantalla Universal con Win32 GDI

En `crates/capture/src/lib.rs`, la captura de pantalla se implementa invocando las APIs canónicas del subsistema gráfico de Windows:

1. `GetDC(HWND(0))` o `CreateDCW`: Obtiene el contexto de dispositivo (Device Context) del escritorio completo o del monitor seleccionado mediante `EnumDisplayMonitors`.
2. `CreateCompatibleDC` y `CreateCompatibleBitmap`: Asigna un mapa de bits en memoria con el ancho y alto del monitor.
3. `SelectObject` y `BitBlt`: Copia el bloque de píxeles desde la pantalla hacia el búfer compatible mediante transferencia de bloques de bits acelerada por el kernel de Windows.
4. `GetDIBits`: Extrae la matriz lineal de bytes en formato BGRA 32-bit lista para ser entregada al compresor JPEG.

---

## 2. Inyección de Periféricos con `SendInput`

En `crates/input/src/lib.rs` y `WindowsInputInjectionService.cs`:

* **Estructura `INPUT`:** Win32 requiere que todos los eventos de ratón y teclado se empaqueten dentro de uniones C `INPUT`.
* **Movimiento Absoluto en Pantalla:**
  ```c
  INPUT input = {0};
  input.type = INPUT_MOUSE;
  input.mi.dx = (LONG)(normalized_x * 65535.0f);
  input.mi.dy = (LONG)(normalized_y * 65535.0f);
  input.mi.dwFlags = MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE;
  SendInput(1, &input, sizeof(INPUT));
  ```
* **Integración con DPI Alto (Per-Monitor High-DPI):**  
  NxDesk habilita en su manifiesto `app.manifest` la directiva `dpiAwareness = PerMonitorV2`, asegurando que las coordenadas no sufran distorsión cuando el usuario utiliza escalados de Windows al 125%, 150% o 200%.
