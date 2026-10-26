# Frontend Excellence & UX Governance Rules

Aplica a desarrollo de interfaces web y desktop (React, Angular, Vue, Svelte, Blazor, WPF/Avalonia, Vanilla HTML/CSS).

## Reglas Obligatorias (ALWAYS DO)
1. **Aislamiento de Estado y Componentes Reutilizables:** Separar componentes presentacionales (dumb/stateless) de componentes contenedores (smart/stateful). El estado global debe ser predecible e inmutable.
2. **Diseño Visual de Primer Nivel (Rich Aesthetics):**
   - Utilizar paletas de color curadas con armonía (modo oscuro/claro por tokens semánticos).
   - Tipografía moderna cargada con fallback tipográfico adecuado.
   - Micro-animaciones y transiciones suaves (`ease-in-out`, duraciones entre 150ms-300ms).
   - Efectos visuales modernos (glassmorphism sutil, bordes pulidos, sombras dinámicas).
3. **Accesibilidad (WCAG 2.1 AA):**
   - Contraste de texto y elementos interactivos mínimo de 4.5:1.
   - Todo elemento interactivo DEBE ser accesible mediante teclado (`tabindex`, focus ring visible).
   - Etiquetas semánticas `aria-label` y roles en controles personalizados.
4. **Resiliencia & Performance:**
   - Cero Cumulative Layout Shift (CLS): definir dimensiones explícitas en imágenes y contenedores dinámicos.
   - Estados de carga (Skeleton loaders o spinners sutiles) y estados de error claros con botón de reintento.

## Reglas Prohibidas (NEVER DO)
- ❌ **NEVER** usar estilos inline `style="..."` dispersos; utilizar variables CSS semánticas o clases utilitarias del design system.
- ❌ **NEVER** bloquear la interacción del usuario sin feedback visual inmediato (< 100ms de respuesta).
- ❌ **NEVER** diseñar interfaces genéricas o sin pulido visual.
