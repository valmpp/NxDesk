# 🔐 Seguridad: Gestión de Secretos y Variables Sensibles — NxDesk

| Código | SEC-SEC-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Security Architecture Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. Principio de Cero Secretos en Repositorio

Ningún secreto, contraseña o clave criptográfica reside en el código fuente de NxDesk ni en sus archivos de configuración rastreados por Git. El archivo `.gitignore` excluye obligatoriamente:
* `.env` y `.env.production`
* Certificados privados (`*.key`, `*.pem`, `*.pfx`)
* Búferes temporales de video o transferencias (`*.mp4`, `NxDesk_Transfers/`)

---

## 2. Inyección de Secretos en Entornos de Producción

* **En Servicios de Windows:** Mediante parámetros protegidos en la bóveda de NSSM o variables de entorno de nivel `SYSTEM`.
* **En Servicios de Linux:** Mediante archivos `EnvironmentFile=/etc/nxdesk/secrets.env` con permisos restringidos `chmod 600`.
* **En Docker:** Mediante Docker Secrets o montajes de volumen de solo lectura hacia certificados TLS gestionados por certbot/Let's Encrypt.
