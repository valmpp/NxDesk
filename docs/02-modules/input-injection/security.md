# 🛡️ Seguridad y Control de Autorización en Inyección de Entrada — NxDesk

| Código | MOD-INP-002 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Security Architecture Team |
| Estado | Aprobado | Clasificación | Documentación Técnica Oficial |

---

## 1. La Amenaza: Inyección de Input Arbitrario (CWE-20 / CWE-285)

Si un cliente remoto puede enviar paquetes de red `SignalingEvent::Input` y el sistema operativo anfitrión los inyecta ciegamente sin comprobar si la sesión fue aprobada por el usuario local o si tiene permiso de escritura, cualquier atacante en la red podría controlar el cursor, abrir ventanas de terminal y ejecutar código sin consentimiento.

---

## 2. El Filtro Defensivo en `main.rs`

En `nxdesk-rust/apps/client/src/main.rs`, la inyección directa fue reemplazada por una verificación atómica sobre el estado de la sesión activa:

```rust
SignalingEvent::Input(input_evt) => {
    // Verificación estricta de sesión activa y autorización concedida
    let can_inject = {
        let active = state_events.active_session.read().await;
        active.as_ref().is_some_and(|s| s.allow_input)
    };

    if can_inject {
        let injector = state_events.input_injector.lock().await;
        injector.inject(&input_evt);
    } else {
        warn!("⛔ [Seguridad] Input descartado: no existe sesión activa con permiso 'allow_input'.");
    }
}
```

---

## 3. Matriz de Concesión de Permisos

| Escenario de Conexión | Estado de `allow_input` | Comportamiento del Inyector |
| :--- | :---: | :--- |
| **Sesión no iniciada** (nadie conectado) | `false` | Todos los eventos son descartados y auditados en el log. |
| **Rol `ViewOnly`** (Solo Visualización) | `false` | Se transmite video pero el ratón y teclado remoto son ignorados. |
| **Rol `FileOnly`** (Solo Archivos) | `false` | Entrada bloqueada; solo se procesan transferencias de ficheros. |
| **Rol `FullControl`** (Aprobado por el Host) | `true` | Inyección activa de teclado y ratón en el sistema operativo. |
| **Host pulsa "Desconectar"** | `false` | Revocación inmediata: cualquier input subsiguiente es bloqueado. |
