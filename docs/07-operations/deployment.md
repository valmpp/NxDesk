# 🚀 Operaciones: Guía de Despliegue — NxDesk

| Código | OPS-DEP-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Operations Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Requisitos Previos de Despliegue
* Servidor con Linux (Ubuntu 22.04 LTS / Debian 12 / RHEL 9) o Windows Server 2022.
* Puertos abiertos en Firewall:
  * `TCP 5000`: Señalización WebSocket (o `TCP 443` si se utiliza Nginx con TLS).
  * `UDP 5001`: Descubrimiento en LAN (solo dentro de la misma subred).

---

## 2. Procedimiento de Despliegue del Servidor (Paso a Paso)

```bash
# 1. Compilación del binario release optimizado
cd nxdesk-rust
cargo build --release -p nxdesk-server

# 2. Copia del binario a /usr/local/bin
sudo cp target/release/nxdesk-server /usr/local/bin/

# 3. Instalación de la unidad systemd
sudo cp ../deploy/systemd/nxdesk-server.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now nxdesk-server

# 4. Verificación de salud (Health Check)
curl -I http://localhost:5000/health
# Esperado: HTTP/1.1 200 OK
```

---

## 3. Despliegue del Cliente de Escritorio para Usuarios

1. Compilar el ejecutable de Windows:
   ```powershell
   cargo build --release -p nxdesk-client
   ```
2. El archivo resultante `target/release/nxdesk-client.exe` es un ejecutable completamente autónomo y portable (sin instaladores obligatorios ni dependencias de runtime adicionales).
3. Distribuir junto con el script de exclusión de firewall si es necesario: `permitir-firewall.bat`.
