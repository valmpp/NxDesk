# 🎯 Objetivos del Sistema — NxDesk

| Código | OVR-OBJ-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Architecture Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Objetivos Principales (Core)

1. **Latencia Sub-50ms en Red Local:** Lograr una experiencia fluida de control remoto transmitiendo entre 30 y 60 FPS con retraso imperceptible en redes LAN cableadas o Wi-Fi 5/6.
2. **Consumo Mínimo de Recursos:**
   * Memoria RAM del cliente de escritorio en reposo: `< 65 MB`.
   * Memoria RAM en sesión activa con streaming de video: `< 120 MB`.
   * Cero consumo de motor de renderizado web Chromium.
3. **Autonomía Operativa Completa:** Operar sin internet externo mediante radar UDP en el puerto `5001` y servidor de señalización local autónomo.
4. **Seguridad Robusta por Diseño:**
   * Diálogo modal de consentimiento explícito previo en la pantalla del anfitrión (`Host`).
   * Sandboxing estricto en transferencia de archivos (`%USERPROFILE%\Downloads\NxDesk_Transfers`).
   * Lista blanca estricta de comandos diagnósticos para la terminal remota.
5. **Portabilidad Multiplataforma:** Ecosistema compatible con Windows 10/11 (DirectX/GDI/SendInput) y Linux (X11/PipeWire/uinput).

---

## 2. Métricas de Calidad de Software (ISO 25010)

| Dimensión | Métrica Objetivo | Estado Actual |
| :--- | :--- | :---: |
| **Rendimiento** | Framerate estable ≥ 30 FPS en resolución 1080p | 🟢 Cumplido |
| **Tiempo de Inicio** | Arranque del ejecutable nativo en < 1.2 segundos | 🟢 Cumplido |
| **Estabilidad** | Cero violaciones de memoria (`0xc0000005`) y cero panics | 🟢 Cumplido (Glow OpenGL) |
| **Seguridad** | Cero vulnerabilidades críticas/altas en dependencias y código | 🟢 Cumplido (Auditoría limpia) |
| **Mantenibilidad** | Clean Architecture con separación estricta de capas | 🟢 Cumplido |
