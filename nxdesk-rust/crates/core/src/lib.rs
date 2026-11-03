use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Estado de conexión o disponibilidad de un par (Peer)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub enum PeerStatus {
    Offline,
    #[default]
    Background,
    Connected,
}

/// Identidad del dispositivo en la red (compatible con el formato JSON de C# NxDesk)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PeerIdentity {
    pub id: String,
    #[serde(default)]
    pub alias: String,
    #[serde(default)]
    pub ip_address: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub status: PeerStatus,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub os: Option<String>,
}

const fn default_port() -> u16 {
    5000
}

/// Roles de sesión y privilegios de control remoto
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum SessionRole {
    ViewOnly,
    FullControl,
    Administrator,
}

/// Solicitud de autenticación
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthRequest {
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_alias: String,
    pub password: Option<String>,
    pub requested_role: Option<SessionRole>,
}

/// Respuesta de autenticación
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    pub success: bool,
    pub session_token: Option<String>,
    pub granted_role: SessionRole,
    pub error_message: Option<String>,
}

/// Información geométrica y lógica de una pantalla o monitor
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ScreenInfoDto {
    pub index: i32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub is_primary: bool,
}

/// Configuración de la aplicación cliente
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub device_id: String,
    pub device_alias: String,
    pub access_password: Option<String>,
    pub allow_remote_terminal: bool,
    pub web_port: u16,
    pub auto_share: bool,
    pub signaling_server_url: String,
    pub max_fps: u32,
    pub quality: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            device_id: "111-222-333".to_string(),
            device_alias: "NxDesk-Host".to_string(),
            access_password: None,
            allow_remote_terminal: false,
            web_port: 8080,
            auto_share: true,
            signaling_server_url: "ws://127.0.0.1:5000/signalinghub".to_string(),
            max_fps: 60,
            quality: 75,
        }
    }
}

/// Botones del ratón soportados
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Tipos de eventos de entrada (ratón y teclado) compatibles con InputEventDto
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "Type", rename_all = "PascalCase")]
pub enum InputEvent {
    MouseMove {
        #[serde(rename = "X")]
        x: f32, // Coordenadas normalizadas (0.0 a 1.0)
        #[serde(rename = "Y")]
        y: f32,
    },
    MouseDown {
        #[serde(rename = "MouseButton")]
        button: MouseButton,
        #[serde(rename = "X", default)]
        x: f32,
        #[serde(rename = "Y", default)]
        y: f32,
    },
    MouseUp {
        #[serde(rename = "MouseButton")]
        button: MouseButton,
        #[serde(rename = "X", default)]
        x: f32,
        #[serde(rename = "Y", default)]
        y: f32,
    },
    MouseWheel {
        #[serde(rename = "DeltaX", default)]
        delta_x: f32,
        #[serde(rename = "DeltaY", default)]
        delta_y: f32,
    },
    KeyDown {
        #[serde(rename = "KeyCode")]
        key_code: i32,
        #[serde(rename = "KeyChar", default)]
        key_char: Option<String>,
    },
    KeyUp {
        #[serde(rename = "KeyCode")]
        key_code: i32,
    },
}

/// Mensaje de señalización intercambiado sobre WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SignalingMessage {
    #[serde(rename = "Type")]
    pub msg_type: String,
    pub payload: String,
    pub byte_data: Option<Vec<u8>>,
    pub sender_id: Option<String>,
    pub sender_alias: Option<String>,
    pub metadata: Option<HashMap<String, String>>,
}

impl SignalingMessage {
    pub fn new(msg_type: &str, payload: impl Into<String>) -> Self {
        Self {
            msg_type: msg_type.to_string(),
            payload: payload.into(),
            byte_data: None,
            sender_id: None,
            sender_alias: None,
            metadata: None,
        }
    }

    pub fn new_frame(data: Vec<u8>) -> Self {
        Self {
            msg_type: "frame".to_string(),
            payload: String::new(),
            byte_data: Some(data),
            sender_id: None,
            sender_alias: None,
            metadata: None,
        }
    }

    pub fn new_input(event: &InputEvent) -> Result<Self, serde_json::Error> {
        let payload = serde_json::to_string(event)?;
        Ok(Self {
            msg_type: "input".to_string(),
            payload,
            byte_data: None,
            sender_id: None,
            sender_alias: None,
            metadata: None,
        })
    }
}

/// DTO para solicitud de transferencia de archivo
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct FileTransferRequest {
    pub transfer_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub target_directory: String,
}

/// DTO para fragmento binario de transferencia de archivo
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct FileChunk {
    pub transfer_id: String,
    pub offset: u64,
    pub is_last: bool,
    pub data: Vec<u8>,
}

/// DTO de consola o terminal remota
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TerminalMessage {
    pub session_id: String,
    pub data: String,
    pub is_error: bool,
}

/// Elemento dentro de un directorio remoto
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DirectoryItemDto {
    pub name: String,
    pub is_directory: bool,
    pub size: u64,
    pub last_modified: Option<String>,
}

/// Respuesta de listado de directorio
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ListDirectoryResponse {
    pub current_path: String,
    pub items: Vec<DirectoryItemDto>,
    pub error: Option<String>,
}

/// Solicitud de sesión remota al estilo AnyDesk (ConnectionTypeDialog)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SessionRequestDto {
    pub request_id: String,
    pub client_id: String,
    pub client_alias: String,
    #[serde(default = "default_session_role")]
    pub requested_role: SessionRole,
    #[serde(default = "default_true")]
    pub allow_input: bool,
    #[serde(default = "default_true")]
    pub allow_clipboard: bool,
    #[serde(default)]
    pub allow_file_transfer: bool,
    #[serde(default)]
    pub timestamp: u64,
}

fn default_session_role() -> SessionRole {
    SessionRole::FullControl
}

fn default_true() -> bool {
    true
}

/// Decisión del anfitrión sobre la solicitud de sesión entrante (Aceptar / Rechazar)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SessionDecisionDto {
    pub request_id: String,
    pub accept: bool,
    pub granted_role: Option<SessionRole>,
    #[serde(default = "default_true")]
    pub allow_input: bool,
    #[serde(default = "default_true")]
    pub allow_clipboard: bool,
    #[serde(default)]
    pub allow_file_transfer: bool,
    pub rejection_reason: Option<String>,
}

/// Respuesta de sesión remota entregada al cliente solicitante
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SessionResponseDto {
    pub success: bool,
    pub request_id: String,
    pub session_token: Option<String>,
    pub granted_role: SessionRole,
    pub allow_input: bool,
    pub allow_clipboard: bool,
    pub allow_file_transfer: bool,
    pub error_message: Option<String>,
}

/// Información de la sesión activa en el anfitrión (HostDisconnectWindow)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ActiveSessionInfo {
    pub session_id: String,
    pub peer_id: String,
    pub peer_alias: String,
    pub granted_role: SessionRole,
    pub allow_input: bool,
    pub allow_clipboard: bool,
    pub allow_file_transfer: bool,
    pub start_time: u64,
}

/// Máquina de estados canónica de la sesión remota.
/// IDLE → CONNECTING → CONNECTED → DISCONNECTING → IDLE
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub enum SessionState {
    #[default]
    Idle,
    Connecting,
    Connected,
    Disconnecting,
}

/// Desglose de latencia por etapa (ms), calculado en el visor.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LatencyBreakdown {
    pub capture_ms: f32,
    pub encode_ms: f32,
    /// Estimación de tránsito de red = RTT/2 (inmune al desfase de relojes entre equipos)
    pub network_ms: f32,
    pub decode_ms: f32,
    /// Medio intervalo de refresco de la UI (latencia media de presentación)
    pub render_ms: f32,
    pub total_ms: f32,
}

/// Protocolo de control de sesión y terminal (viaja por el canal de control, separado del video).
pub mod protocol {
    use serde::{Deserialize, Serialize};

    pub const SESSION_REQUEST: &str = "session_request";
    pub const SESSION_DECISION: &str = "session_decision";
    /// BYE explícito: cualquiera de los dos extremos termina la sesión
    pub const SESSION_END: &str = "session_end";
    pub const SESSION_END_ACK: &str = "session_end_ack";
    /// Heartbeat (1 Hz) y su respuesta; también miden RTT
    pub const HEARTBEAT: &str = "ping_rtt";
    pub const HEARTBEAT_ACK: &str = "pong_rtt";
    pub const PEER_OFFLINE: &str = "peer_offline";

    pub const TERMINAL_OPEN: &str = "terminal_open";
    pub const TERMINAL_OPENED: &str = "terminal_opened";
    pub const TERMINAL_DENIED: &str = "terminal_denied";
    /// Salida del PTY (host → visor)
    pub const TERMINAL_DATA: &str = "terminal_data";
    /// Pulsaciones del usuario (visor → host)
    pub const TERMINAL_INPUT: &str = "terminal_input";
    pub const TERMINAL_RESIZE: &str = "terminal_resize";
    /// Cierre o revocación (cualquier extremo)
    pub const TERMINAL_CLOSE: &str = "terminal_close";

    /// Sin heartbeat del par durante este tiempo ⇒ par caído ⇒ sesión terminada
    pub const HEARTBEAT_TIMEOUT_MS: u64 = 6_000;
    pub const END_ACK_TIMEOUT_MS: u64 = 3_000;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "PascalCase")]
    pub struct SessionEndDto {
        pub from: String,
        pub to: String,
        pub reason: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "PascalCase")]
    pub struct TerminalOpenDto {
        pub from: String,
        pub to: String,
        pub cols: u16,
        pub rows: u16,
    }

    /// Mensaje genérico de terminal dirigido (data / input / resize / close / opened / denied)
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
    #[serde(rename_all = "PascalCase")]
    pub struct TerminalMsgDto {
        pub from: String,
        pub to: String,
        #[serde(default)]
        pub data: String,
        #[serde(default)]
        pub cols: u16,
        #[serde(default)]
        pub rows: u16,
    }
}

/// Cabecera binaria de timing antepuesta a cada fotograma JPEG enviado por la red.
/// Formato (20 bytes, little-endian): `b"NXV1" | send_ts_ms:u64 | capture_us:u32 | encode_us:u32`
pub mod frame_header {
    pub const MAGIC: &[u8; 4] = b"NXV1";
    pub const LEN: usize = 24;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct FrameTiming {
        pub send_ts_ms: u64,
        pub capture_us: u32,
        pub encode_us: u32,
        pub seq: u32,
    }

    pub fn wrap(payload: &[u8], t: FrameTiming) -> Vec<u8> {
        let mut out = Vec::with_capacity(LEN + payload.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&t.send_ts_ms.to_le_bytes());
        out.extend_from_slice(&t.capture_us.to_le_bytes());
        out.extend_from_slice(&t.encode_us.to_le_bytes());
        out.extend_from_slice(&t.seq.to_le_bytes());
        out.extend_from_slice(payload);
        out
    }

    /// Devuelve (timing, payload). Compatible con fotogramas antiguos sin cabecera.
    pub fn unwrap(buf: &[u8]) -> (Option<FrameTiming>, &[u8]) {
        if buf.len() > LEN && &buf[..4] == MAGIC {
            let send_ts_ms = u64::from_le_bytes(buf[4..12].try_into().unwrap_or_default());
            let capture_us = u32::from_le_bytes(buf[12..16].try_into().unwrap_or_default());
            let encode_us = u32::from_le_bytes(buf[16..20].try_into().unwrap_or_default());
            let seq = u32::from_le_bytes(buf[20..24].try_into().unwrap_or_default());
            (
                Some(FrameTiming { send_ts_ms, capture_us, encode_us, seq }),
                &buf[LEN..],
            )
        } else {
            (None, buf)
        }
    }
}

#[cfg(test)]
mod protocol_tests {
    use super::*;

    #[test]
    fn frame_header_roundtrip() {
        let jpeg = [0xFFu8, 0xD8, 1, 2, 3];
        let t = frame_header::FrameTiming { send_ts_ms: 42, capture_us: 1500, encode_us: 9000, seq: 1 };
        let wrapped = frame_header::wrap(&jpeg, t);
        let (got, payload) = frame_header::unwrap(&wrapped);
        assert_eq!(got, Some(t));
        assert_eq!(payload, &jpeg);
    }

    #[test]
    fn frame_header_legacy_passthrough() {
        let jpeg = [0xFFu8, 0xD8, 0xFF, 0xE0];
        let (got, payload) = frame_header::unwrap(&jpeg);
        assert!(got.is_none());
        assert_eq!(payload, &jpeg);
    }

    #[test]
    fn session_end_serializes_pascal_case() {
        let dto = protocol::SessionEndDto { from: "a".into(), to: "b".into(), reason: "user".into() };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("\"From\":\"a\"") && json.contains("\"To\":\"b\""));
    }
}
