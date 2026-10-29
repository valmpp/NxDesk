# ⚠️ Problema Conocido: Incompatibilidad de Drivers Vulkan en Windows — NxDesk

| Código | INC-KNW-001 | Versión | v1.0 |
|--------|-------------|---------|------|
| Actualizado | 2026-10-28 | Autor | NxDesk Low-Level Engineering |
| Estado | Mitigado Definitivamente | Clasificación | Documentación de Problemas Conocidos |

---

## Descripción del Conflicto
Múltiples herramientas populares en Windows (OBS Studio, Discord Game Overlay, RivaTuner Statistics Server, Geforce Experience) inyectan capas de Vulkan en el registro del sistema (`HKEY_LOCAL_MACHINE\SOFTWARE\Khronos\Vulkan\ImplicitLayers`).  
Cuando una aplicación basada en `wgpu` arranca, intenta inicializar estas capas implícitas. Si alguna capa está corrupta, desactualizada o pertenece a una versión de 32 bits, el proceso sufre una violación de acceso `0xc0000005` incontrolable por el código de usuario.

---

## Estrategia de Mitigación de NxDesk
NxDesk no depende de Vulkan. Toda la canalización gráfica de la aplicación de escritorio nativa utiliza **OpenGL clásico vía Glow**, garantizando que ninguna capa implícita de Vulkan sea cargada en el espacio de memoria del proceso.
