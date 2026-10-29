# ⚡ Referencia Rápida: Códigos de Error y Diagnóstico — NxDesk

| Código | REF-ERR-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Quality & Testing Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## Catálogo de Códigos de Error Comunes

| Código / Error | Componente | Causa Raíz | Acción Recomendada |
| :--- | :--- | :--- | :--- |
| `0xc0000005` | `Client Window Init` | Fallo de segmentación por sondeo de adaptadores Vulkan en Windows. | Asegurar que `eframe` use `features = ["glow"]` y `Renderer::Glow`. Ver [KB-TRB-001](../../10-knowledge-base/troubleshooting/0xc0000005-access-violation.md). |
| `401 Unauthorized` | `Signaling / Auth` | Contraseña de acceso no asistido incorrecta o token caducado. | Verificar `--password` o solicitar aprobación manual en pantalla. |
| `403 Forbidden` | `FileTransfer / Terminal`| Intento de acceso a ficheros fuera del sandbox o comando no autorizado. | Comprobar que la ruta esté en `NxDesk_Transfers` y el comando en lista blanca. |
| `Room Full (429/Hub)` | `SignalingHub` | La sala ha alcanzado el límite máximo de 5 conexiones simultáneas. | Desconectar clientes inactivos o crear una sala nueva. |
| `WSAEADDRINUSE (10048)` | `Discovery / Server` | El puerto 5000 (TCP) o 5001 (UDP) ya está ocupado por otra instancia. | Cerrar instancias huérfanas en segundo plano (`Get-Process nxdesk* \| Stop-Process`). |
