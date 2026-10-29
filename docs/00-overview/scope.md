# 📌 Alcance del Sistema: Qué Hace y Qué NO Hace — NxDesk

| Código | OVR-SCP-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Product Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Lo que el Sistema SÍ Hace (In-Scope)

* ✅ **Streaming de Pantalla en Tiempo Real:** Captura continua de monitores principales o secundarios y transmisión de frames JPEG acelerados por GPU.
* ✅ **Control Remoto de Periféricos:** Transmisión e inyección segura de eventos de ratón (movimiento absoluto y relativo, clics, rueda de desplazamiento) y teclado (pulsaciones, modificadores y cadenas de texto).
* ✅ **Conmutación Dinámica de Monitores:** Capacidad de seleccionar el monitor físico a transmitir durante la sesión sin desconectarse.
* ✅ **Handshake y Aprobación de Sesión AnyDesk:** Flujo de solicitud interactiva con asignación granular de roles (`FullControl`, `ViewOnly`, `FileOnly`) y widget flotante de desconexión rápida (`HostDisconnectWindow`).
* ✅ **Transferencia Segura de Archivos:** Lectura y escritura de bloques fragmentados (Chunks) confinados obligatoriamente dentro del directorio sandbox `%USERPROFILE%\Downloads\NxDesk_Transfers`.
* ✅ **Terminal Remota de Diagnóstico:** Ejecución remota de comandos de diagnóstico restringidos a una lista blanca (`dir`, `ping`, `ipconfig`, `whoami`, etc.).
* ✅ **Descubrimiento Automático en Red Local:** Anuncio y detección de pares en LAN mediante balizas UDP Broadcast en el puerto `5001`.
* ✅ **Servidor de Señalización Autónomo:** Conexión y retransmisión de salas de par a par a través de WebSockets con delimitador `0x1E`.

---

## 2. Lo que el Sistema NO Hace (Out-of-Scope)

* ❌ **No es una Aplicación Web para Navegadores:** No incluye dashboard web en Chromium ni requiere abrir puertos HTTP en el navegador local (`localhost:8080`). Es una aplicación de escritorio nativa pura.
* ❌ **No Almacena Credenciales en la Nube:** No existe una base de datos central en la nube que guarde contraseñas, telemetría o grabaciones de los usuarios.
* ❌ **No Accede al Disco sin Sandbox:** No permite descargar ni explorar carpetas del sistema operativo fuera del directorio de transferencias seguro (`C:\Windows`, `C:\Program Files`, etc.).
* ❌ **No Ejecuta Comandos Arbitrarios del Sistema Operativo:** No permite ejecutar scripts destructivos (`rmdir /s /q`, `format`, `powershell -enc`) desde la terminal remota.
* ❌ **No Permite Conexiones Sin Autorización (Modo No Supervisado sin Configurar):** Requiere contraseña de acceso no asistido explícita o la aceptación manual del usuario anfitrión frente a la pantalla.
