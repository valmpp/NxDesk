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
        #[serde(rename = "KeyCode", default)]
        key_code: Option<String>,
        #[serde(rename = "Key", default)]
        key: u32,
    },
    KeyUp {
        #[serde(rename = "KeyCode", default)]
        key_code: Option<String>,
        #[serde(rename = "Key", default)]
        key: u32,
    },
}

/// Mensaje de señalización intercambiado entre pares (compatible con SdpMessage de C#)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct SignalingMessage {
    #[serde(rename = "Type")]
    pub msg_type: String, // "offer", "answer", "frame", "input", "terminal", "file_req", "file_chunk"
    #[serde(default)]
    pub payload: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_data: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_alias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

impl SignalingMessage {
    pub fn new(msg_type: impl Into<String>, payload: impl Into<String>) -> Self {
        Self {
            msg_type: msg_type.into(),
            payload: payload.into(),
            byte_data: None,
            sender_id: None,
            sender_alias: None,
            metadata: None,
        }
    }

    pub fn new_frame(bytes: Vec<u8>) -> Self {
        Self {
            msg_type: "frame".to_string(),
            payload: String::new(),
            byte_data: Some(bytes),
            sender_id: None,
            sender_alias: None,
            metadata: None,
        }
    }

    pub fn new_input(event: &InputEvent) -> Result<Self, serde_json::Error> {
        let json = serde_json::to_string(event)?;
        Ok(Self {
            msg_type: "input".to_string(),
            payload: json,
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