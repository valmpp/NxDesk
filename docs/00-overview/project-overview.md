# 🌐 Visión General del Proyecto — NxDesk

| Código | OVR-PROJ-001 | Versión | v1.0 |
|--------|--------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Product & Architecture Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. ¿Qué es NxDesk?
**NxDesk** es una solución soberana y autónoma de **escritorio remoto, asistencia técnica y streaming de ultra-baja latencia**. Diseñada con una arquitectura nativa de escritorio híbrida (Rust y C# .NET), permite controlar equipos remotos, transferir archivos, ejecutar diagnósticos y compartir pantallas a través de redes locales (LAN) e Internet sin depender de infraestructura en la nube propietaria de terceros.

---

## 2. El Problema que Resuelve
1. **Dependencia y Costo de Soluciones Comerciales:** Herramientas como AnyDesk o TeamViewer imponen límites de sesión, suscripciones costosas y dependencia absoluta de sus servidores de señalización en la nube.
2. **Privacidad y Soberanía de Datos:** En entornos corporativos o industriales, el tráfico de captura de pantalla y teclado no debe salir de la intranet de la organización.
3. **Latencia y Overhead Web:** Las interfaces basadas en navegadores web (WebRTC en browser o Electron) consumen cantidades desproporcionadas de memoria RAM y sufren de latencias de renderizado inaceptables para control remoto de precisión.
4. **Resiliencia Operativa:** NxDesk funciona tanto con un servidor de señalización liviano autoalojado como mediante descubrimiento automático directo en red local por paquetes UDP Broadcast (puerto 5001).

---

## 3. Pilares Tecnológicos
* **Ecosistema Rust:**
  * **Cliente de Escritorio Nativo:** Construido sobre `egui` y `eframe` con aceleración por hardware `Glow (OpenGL)`.
  * **Motor de Captura de Pantalla:** Interoperabilidad nativa con Win32 GDI y compresión JPEG multihilo de alta tasa.
  * **Inyección de Entrada:** Win32 `SendInput` con escalado dinámico de coordenadas.
  * **Concurrencia Asíncrona:** Runtime Tokio multihilo con canales `broadcast` y `mpsc` de baja latencia.
* **Ecosistema C# (.NET 10.0):**
  * **Cliente GUI Avalonia:** Interfaz Fluent con enlace de datos compilado (CompiledBindings) y patrón MVVM.
  * **Captura de Pantalla por GPU:** SharpDX (DirectX 11 DXGI Desktop Duplication API) para 60 FPS estables.
  * **Infraestructura SignalR:** Comunicación cliente-servidor mediante protocolo SignalR de alta velocidad.

---

## 4. Filosofía de Diseño
* **Zero Browser Overhead:** Aplicaciones de escritorio ejecutables nativas y livianas (sin Chromium, sin Node.js en producción).
* **Paridad Visual 1:1:** Identidad moderna inspirada en AnyDesk y el tema oscuro de Avalonia (`#1E1E1E`, `#007ACC`).
* **Seguridad por Defecto:** Consentimiento explícito obligatorio en pantalla antes de otorgar control de periféricos o acceso al sistema de archivos.
