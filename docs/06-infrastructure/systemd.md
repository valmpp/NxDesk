# 🐧 Infraestructura: Servicios de Linux (Systemd) — NxDesk

| Código | INF-SYS-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk DevOps & Infrastructure Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Unidades de Servicio en `deploy/systemd/`

* `nxdesk-server.service`: Demonio del servidor de señalización.
* `nxdesk-client.service`: Demonio del cliente anfitrión en modo desatendido.

---

## 2. Instalación y Puesta en Marcha

```bash
# 1. Copiar los archivos de unidad al directorio del sistema
sudo cp deploy/systemd/nxdesk-server.service /etc/systemd/system/

# 2. Recargar demonios de systemd
sudo systemctl daemon-reload

# 3. Habilitar inicio automático al boot y arrancar el servicio
sudo systemctl enable --now nxdesk-server.service

# 4. Verificar estado y logs de ejecución
sudo systemctl status nxdesk-server.service
sudo journalctl -u nxdesk-server.service -f
```

---

## 3. Parámetros de Seguridad en el Servicio

La unidad incluye endurecimiento (hardening) de proceso de Linux:
* `NoNewPrivileges=true`: Impide la escalada de privilegios a través de binarios setuid.
* `ProtectSystem=full`: Monta `/usr`, `/boot` y `/etc` en modo solo lectura para el proceso.
* `Restart=always` con `RestartSec=3s`: Garantiza alta disponibilidad y recuperación ante fallas.
