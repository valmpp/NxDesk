use nxdesk_core::{AuthRequest, AuthResponse, SessionRole};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Gestor de autenticación y sesiones de control remoto
#[derive(Clone, Default)]
pub struct AuthManager {
    access_password: Arc<RwLock<Option<String>>>,
    // session_token -> SessionRole
    #[allow(dead_code)]
    active_tokens: Arc<RwLock<HashMap<String, SessionRole>>>,
}

impl AuthManager {
    pub fn new(access_password: Option<String>) -> Self {
        Self {
            access_password: Arc::new(RwLock::new(access_password)),
            active_tokens: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Actualiza la contraseña de acceso requerida
    #[allow(dead_code)]
    pub async fn set_password(&self, password: Option<String>) {
        let mut lock = self.access_password.write().await;
        *lock = password;
    }

    /// Autentica una solicitud de un cliente remoto
    pub async fn authenticate(&self, req: &AuthRequest) -> AuthResponse {
        let expected_pw = self.access_password.read().await;

        if let Some(ref required_pw) = *expected_pw {
            match &req.password {
                Some(provided) if provided == required_pw => {
                    let token = generate_secure_token();
                    let role = req.requested_role.unwrap_or(SessionRole::FullControl);
                    let mut tokens = self.active_tokens.write().await;
                    tokens.insert(token.clone(), role);

                    AuthResponse {
                        success: true,
                        error_message: None,
                        session_token: Some(token),
                        granted_role: role,
                    }
                }
                _ => AuthResponse {
                    success: false,
                    error_message: Some("Contraseña de acceso incorrecta.".to_string()),
                    session_token: None,
                    granted_role: SessionRole::ViewOnly,
                },
            }
        } else {
            // Sin contraseña requerida, se concede el rol solicitado
            let token = generate_secure_token();
            let role = req.requested_role.unwrap_or(SessionRole::FullControl);
            let mut tokens = self.active_tokens.write().await;
            tokens.insert(token.clone(), role);

            AuthResponse {
                success: true,
                error_message: None,
                session_token: Some(token),
                granted_role: role,
            }
        }
    }

    /// Valida si un token cuenta con el rol mínimo necesario
    pub async fn has_role(&self, token: &str, min_role: SessionRole) -> bool {
        let tokens = self.active_tokens.read().await;
        if let Some(role) = tokens.get(token) {
            matches!(
                (role, min_role),
                (SessionRole::Administrator, _)
                    | (SessionRole::FullControl, SessionRole::FullControl)
                    | (SessionRole::FullControl, SessionRole::ViewOnly)
                    | (SessionRole::ViewOnly, SessionRole::ViewOnly)
            )
        } else {
            false
        }
    }

    /// Revoca un token de sesión
    #[allow(dead_code)]
    pub async fn revoke_token(&self, token: &str) {
        let mut tokens = self.active_tokens.write().await;
        tokens.remove(token);
    }
}

/// Generador criptográficamente seguro de tokens de sesión (CSPRNG 192-bit)
fn generate_secure_token() -> String {
    use rand::RngCore;
    use std::fmt::Write;
    let mut bytes = [0u8; 24]; // 192 bits de entropía criptográfica
    rand::thread_rng().fill_bytes(&mut bytes);
    let mut hex = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(hex, "{:02x}", b);
    }
    format!("tok_{}", hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_auth_manager_with_password() {
        let auth = AuthManager::new(Some("AdminSecure123".to_string()));

        // Intento con clave errónea
        let bad_req = AuthRequest {
            client_id: "client-1".to_string(),
            client_alias: "BadGuy".to_string(),
            password: Some("wrong".to_string()),
            requested_role: Some(SessionRole::Administrator),
        };
        let bad_resp = auth.authenticate(&bad_req).await;
        assert!(!bad_resp.success);
        assert!(bad_resp.session_token.is_none());

        // Intento con clave correcta
        let good_req = AuthRequest {
            client_id: "client-1".to_string(),
            client_alias: "LegitAdmin".to_string(),
            password: Some("AdminSecure123".to_string()),
            requested_role: Some(SessionRole::Administrator),
        };
        let good_resp = auth.authenticate(&good_req).await;
        assert!(good_resp.success);
        let token = good_resp.session_token.expect("Token expected");

        assert!(auth.has_role(&token, SessionRole::FullControl).await);
        assert!(auth.has_role(&token, SessionRole::Administrator).await);

        auth.revoke_token(&token).await;
        assert!(!auth.has_role(&token, SessionRole::ViewOnly).await);

        auth.set_password(Some("UpdatedSecret".to_string())).await;
        let update_req = AuthRequest {
            client_id: "client-2".to_string(),
            client_alias: "Admin2".to_string(),
            password: Some("UpdatedSecret".to_string()),
            requested_role: Some(SessionRole::Administrator),
        };
        assert!(auth.authenticate(&update_req).await.success);
    }
}
