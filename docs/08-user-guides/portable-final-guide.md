# 🚀 NxDesk Portable - Manual de la Versión Final

---

## 1. Resumen Ejecutivo de la Versión Final

La versión **NxDesk Portable Final** representa el empaquetado de distribución autónomo y de cero instalación de NxDesk para sistemas operativos Windows (64 bits). 

Permite desplegar clientes y operadores remotos en cualquier máquina sin privilegios de administrador, sin instalación de dependencias y con conexión automática al servidor central de señalización en Docker (`ws://10.10.40.11:5000/signalinghub`).

### 📦 Archivos Distribuidos (`dist/NxDesk-Portable-Final.zip`)
| Archivo | Función |
| :--- | :--- |
| `iniciar-nxdesk.bat` | **Lanzador principal recomendado**: Abre la aplicación mostrando la consola con telemetría en tiempo real (FPS, RTT, códec, diagnóstico de cuellos de botella). |
| `iniciar-segundoplano.bat` | Lanzador silencioso en segundo plano sin ventana de comandos. |
| `nxdesk-tauri.exe` | Cliente gráfico moderno (diseño Obsidian Dark con aceleración por hardware). |
| `nxdesk-client.exe` | Motor nativo ultraligero (egui + decodificador TurboJPEG/SIMD) de alto rendimiento. |
| `nxdesk-server.exe` | Servidor de señalización opcional para pruebas locales aisladas sin Docker. |
| `config.json` | Configuración central (URL de señalización, calidad de compresión y límite dinámico de FPS). |
| `permitir-firewall.bat` | Script auxiliar para abrir reglas del Firewall de Windows en redes restringidas. |
| `WebView2Loader.dll` | Librería nativa para entornos Windows sin runtime global de WebView2. |
| `LEEME_INSTRUCCIONES.txt` | Guía de uso rápido para usuarios finales. |

---

## 2. Arquitectura de Red y Protocolo Implementado

### ⚡ Transporte P2P UDP Directo (Prioritario)
* El tráfico masivo de pantalla (fotogramas JPEG empaquetados) y los eventos de ratón (`NXINP`) se transmiten de forma **directa entre computadoras mediante datagramas UDP**, sin pasar por el servidor Docker.
* **Búferes de red ampliados a 4 MB:** Evitan pérdidas de paquetes y desbordamientos en ráfagas de vídeo intensivas.
* **Control de flujo (Pacing) adaptativo:** Cede microsegundos cada 12 fragmentos para drenar el búfer de socket de Windows y evitar bloqueos `WouldBlock`.
* **Latencia física en LAN:** RTT de **1 a 3 ms** con tiempo de red unidireccional de **1.5 ms**.

### 🎯 Enrutamiento Multi-Máquina Exclusivo (`target_id`)
* En entornos con 3 o más computadoras conectadas simultáneamente, la solicitud de conexión (`session_request`) y la decisión (`session_decision`) viajan **únicamente al equipo destino**.
* Ningún equipo ajeno a la sesión recibe diálogos emergentes de confirmación no solicitados.

### 🔄 Gobernador Dinámico de FPS
El motor adapta la cadencia de captura y codificación en tiempo real:
1. **Modo Reposo (8 FPS):** En pantalla estática sin actividad de ratón/teclado. Reduce el consumo de CPU y GPU en más del 70% y lleva el tráfico de red a prácticamente cero.
2. **Modo Normal (30 FPS):** Al interactuar con el sistema, abrir programas o navegar por menús.
3. **Modo Multimedia (60 FPS):** Al reproducir vídeos o arrastrar ventanas con movimiento continuo.

### 🛑 Ciclo de Desconexión Limpia y Reseteo del Puente
* **Notificación garantizada:** Al pulsar "Desconectar", la máquina emite un mensaje `session_end` con `target_id` directo al anfitrión antes de purgar el estado local.
* **Reseteo de socket UDP (`reset_p2p()`):** Cancela de inmediato los hilos de keepalive (pings) de fondo y borra la IP remota asociada.
* **Cero residuos:** Permite conectarse a una segunda máquina inmediatamente sin que los sockets queden bloqueados ni se produzcan caídas de FPS.

---

## 3. Instrucciones de Despliegue y Pruebas

1. **Descarga y extracción:**
   Copiar `dist/NxDesk-Portable-Final.zip` a cualquier máquina Windows y descomprimirlo.
2. **Ejecución:**
   Hacer doble clic en `iniciar-nxdesk.bat`.
3. **Identificación:**
   Cada máquina genera automáticamente un identificador de 9 dígitos (ejemplo: `505-229-743`).
4. **Conexión:**
   Ingresar el ID remoto en el campo de texto y presionar **Conectar**. El anfitrión recibirá la solicitud exclusiva para aceptar o rechazar el control.

