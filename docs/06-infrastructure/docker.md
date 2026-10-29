# 🐳 Infraestructura: Contenedores Docker y Docker Compose — NxDesk

| Código | INF-DOC-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk DevOps & Infrastructure Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Despliegue con Docker Compose (Signaling + Ngrok WAN)

El stack de pruebas de producción de NxDesk orquesta el servidor central de señalización en Rust y un túnel seguro con Ngrok para permitir el acceso a través de Internet público sin abrir puertos en el router:

```yaml
version: '3.8'

services:
  nxdesk-server:
    build:
      context: ../../
      dockerfile: deploy/docker/Dockerfile.server
    container_name: nxdesk-signaling-server
    restart: unless-stopped
    environment:
      - RUST_LOG=info
      - BIND_ADDR=0.0.0.0:5000
    networks:
      - nxdesk-net
    ports:
      - "5000:5000"
    healthcheck:
      test: ["CMD", "curl", "-f", "http://127.0.0.1:5000/health"]
      interval: 10s
      timeout: 5s
      retries: 3

  ngrok:
    image: ngrok/ngrok:latest
    container_name: nxdesk-ngrok-tunnel
    restart: unless-stopped
    command:
      - "http"
      - "nxdesk-server:5000"
    environment:
      - NGROK_AUTHTOKEN=${NGROK_AUTHTOKEN}
    ports:
      - "4040:4040"
    networks:
      - nxdesk-net
    depends_on:
      nxdesk-server:
        condition: service_healthy

networks:
  nxdesk-net:
    driver: bridge
```

---

## 2. Comandos Operativos y Automatización

### Opción A: Automatizado con PowerShell (Recomendado)
```powershell
.\deploy\start-production-test.ps1
```
* Configura `.env`, levanta Docker y consulta automáticamente `http://localhost:4040/api/tunnels` para entregarte la URL pública lista para los clientes `wss://xxxx.ngrok-free.app/signalinghub`.

### Opción B: Comandos Manuales
```bash
# 1. Configurar variables de entorno
cp .env.example .env
# Añade tu NGROK_AUTHTOKEN en .env

# 2. Levantar el stack completo
docker compose up -d --build

# 3. Ver túneles activos de Ngrok
curl http://localhost:4040/api/tunnels

# 4. Logs en tiempo real
docker compose logs -f

# 5. Detener el entorno
docker compose down
```
