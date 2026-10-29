# 🌐 Infraestructura: Reverse Proxy Nginx y Terminación TLS — NxDesk

| Código | INF-NGX-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk DevOps & Infrastructure Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Configuración de Nginx (`deploy/nginx/nxdesk.conf`)

Para entornos de producción expuestos a Internet, se recomienda colocar **Nginx** frente al servidor de señalización para gestionar certificados TLS/SSL (HTTPS/WSS) y aplicar políticas de rate limiting y compresión:

```nginx
map $http_upgrade $connection_upgrade {
    default upgrade;
    ''      close;
}

upstream nxdesk_backend {
    server 127.0.0.1:5000;
    keepalive 32;
}

server {
    listen 443 ssl http2;
    server_name nxdesk.tuempresa.com;

    ssl_certificate     /etc/letsencrypt/live/nxdesk.tuempresa.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/nxdesk.tuempresa.com/privkey.pem;

    # Señalización WebSocket y Handshake
    location /signalinghub {
        proxy_pass http://nxdesk_backend;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection $connection_upgrade;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;

        # Timeouts extendidos para conexiones persistentes
        proxy_read_timeout 86400s;
        proxy_send_timeout 86400s;
    }

    # Health Check
    location /health {
        proxy_pass http://nxdesk_backend/health;
    }
}
```
