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
            max_fps: 30,
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
