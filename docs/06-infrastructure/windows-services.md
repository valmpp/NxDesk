# 🪟 Infraestructura: Servicios de Windows (NSSM) — NxDesk

| Código | INF-WIN-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk DevOps & Infrastructure Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Operación como Servicio en Segundo Plano

Para estaciones de trabajo y servidores que requieren acceso remoto desatendido incluso antes de que un usuario inicie sesión en Windows, NxDesk incluye scripts de instalación basados en **NSSM (Non-Sucking Service Manager)** en `deploy/windows/`:

* `install-service-nssm.ps1`: Registra `nxdesk-client.exe` o `nxdesk-server.exe` como un servicio del sistema.
* `uninstall-service-nssm.ps1`: Detiene y elimina el servicio del Administrador de Servicios de Windows (`services.msc`).

---

## 2. Instalación del Servicio

Ejecutar en una consola de PowerShell con privilegios de Administrador:

```powershell
# Instalar el servicio con inicio automático
powershell -ExecutionPolicy Bypass -File deploy\windows\install-service-nssm.ps1 `
    -ServiceName "NxDeskClient" `
    -BinaryPath "C:\NxDesk\nxdesk-client.exe" `
    -Arguments "--password=ClaveCorporativaSegura --allow-remote-terminal"
```

---

## 3. Configuración y Recuperación Automática

El script configura automáticamente:
* **Tipo de Inicio:** Automático (`SERVICE_AUTO_START`).
* **Acción ante Fallos:** Reinicio del proceso tras 5 segundos (`AppRestartDelay = 5000ms`).
* **Rotación de Logs:** Redirección de `stdout` y `stderr` a `C:\ProgramData\NxDesk\logs\service.log` con rotación a 10 MB.
