[English](OLLAMA_LOCAL.md) | [日本語](OLLAMA_LOCAL.ja.md) | [中文(简体)](OLLAMA_LOCAL.zh-CN.md) | [中文(繁體)](OLLAMA_LOCAL.zh-TW.md) | [한국어](OLLAMA_LOCAL.ko.md) | [Français](OLLAMA_LOCAL.fr.md) | [Deutsch](OLLAMA_LOCAL.de.md) | Español

[← Volver al README de Anthro Bridge](../README.md)

# Claude Code + Ollama Local

Anthro Bridge puede enrutar Claude Code hacia un modelo Ollama ejecutado localmente sin modificar los proveedores en la nube configurados para Claude Desktop y MCP.

---

## Arquitectura

```text
Claude Code
     │
     │ Marcador Claude Code de Anthro Bridge (X-Anthro-Bridge-Client: claude-code)
     ▼
Pasarela Anthro Bridge
     │
     ├─ Ruta Gateway (`active_route = "gateway"`) ──→ Proveedor cloud global (DeepSeek / MiMo / OpenRouter / etc.)
     │
     └─ Ruta Ollama  (`active_route = "ollama"`)  ──→ http://127.0.0.1:11434 (`/v1/messages`)
                                                          │
                                                          ▼
                                                     Modelo local
```

Ollama Local está diseñado exclusivamente para **Claude Code**. No reemplaza al proveedor global en la nube utilizado por Claude Desktop o las herramientas MCP de Anthro Bridge.

---

## Requisitos

1. **Anthro Bridge** instalado y en ejecución.
2. **Ollama** instalado y activo en su máquina local.
3. Al menos un modelo descargado en Ollama (p. ej. `gemma4:latest`, `llama3.3:70b`, `qwen2.5-coder:32b`) o una etiqueta de modelo personalizada válida.
4. Claude Code iniciado mediante el comando generado por Anthro Bridge.

---

## Configurar Ollama Local

En Anthro Bridge:
1. Vaya a **Ajustes > Claves API**.
2. Desplácese hasta la tarjeta de configuración de **Ollama Local** situada debajo de la tabla de claves API.

### Fila principal de ajustes

- **Modelo**: Menú desplegable con los modelos descubiertos y opción personalizada.
- **Actualizar (`🔄 Actualizar`)**: Consulta el punto de conexión local `/api/tags` de Ollama.
- **Thinking**: Seleccione **Normal (Desactivado)** o **Thinking (Activado)**.
- **Mostrar en el panel**: Casilla para controlar la visualización de la tarjeta de Ollama Local en el panel principal.

### Ajustes avanzados (`▸ Ajustes avanzados`)

- **Punto de conexión**: URL base de Ollama (por defecto: `http://127.0.0.1:11434`).
- **Visión (Base64)**: Activa o desactiva la entrada de imágenes en modelos locales multimodales.
- **Ventana de contexto**: Capacidad explícita opcional en tokens (p. ej. `131072`).
- **Sin clave API**: Las instancias locales en loopback de Ollama no requieren clave API.

---

## Descubrimiento y actualización de modelos

Al hacer clic en **Actualizar (`🔄 Actualizar`)** se envía:

```http
GET http://127.0.0.1:11434/api/tags
```

### Reglas de seguridad y preservación

- **Solo loopback**: Únicamente se consultan direcciones de bucle local (`127.0.0.1`, `localhost`, `[::1]`).
- **Manejo no bloqueante de errores**: Si Ollama no está activo o se agota el tiempo de espera (límite de 2 s), se muestra un aviso sin borrar el modelo guardado.
- **Preservación de modelos**: El modelo configurado permanece seleccionado aunque no aparezca en la lista devuelta.
- **Etiquetas personalizadas**: Seleccione `+ Etiqueta de modelo personalizada...` para ingresar cualquier identificador.

---

## Seleccionar Ollama en el panel

1. Active **Mostrar en el panel** en los ajustes.
2. Vaya al **Panel principal**.
3. Haga clic en la tarjeta **Ollama Local**:
   - Establece `claude_code.active_route = "ollama"`.
   - El `active_provider` global (p. ej. `deepseek`) **permanece sin cambios**.
   - La tarjeta de Ollama se resalta como **Activo para Claude Code**.
4. Para regresar a la ruta en la nube:
   - Haga clic en cualquier proveedor en la nube en el panel.
   - Claude Code se restablece automáticamente a `claude_code.active_route = "gateway"`.

---

## Semántica de Thinking

Anthro Bridge traduce su selección al formato compatible con Anthropic requerido por Ollama:

- **Normal**:
  ```json
  "thinking": { "type": "disabled" }
  ```
- **Thinking**:
  ```json
  "thinking": { "type": "enabled" }
  ```

---

## Ventana de contexto y compactación automática

- `context_window` es **opcional**.
- Cuando se especifica, Anthro Bridge lo utiliza para la gestión de contexto y cálculo de auto-compactación en Claude Code.
- Si se omite (`null`), Anthro Bridge no asume un tamaño arbitrario.

---

## Marcador de identificación de Claude Code

El comando generado inyecta un marcador interno mediante `ANTHROPIC_CUSTOM_HEADERS`:

```text
X-Anthro-Bridge-Client: claude-code
```

- **Normalización**: Insensible a mayúsculas/minúsculas, elimina duplicados y preserva encabezados personalizados del usuario.
- **Eliminación previa al reenvío**: Se utiliza solo localmente para el enrutamiento y se elimina antes de reenviar la solicitud.

---

## Aislamiento de clientes

| Cliente | Selector de ruta | Destino |
| :--- | :--- | :--- |
| **Claude Code** | `claude_code.active_route` (`"gateway"` o `"ollama"`) | Proveedor del panel / Ollama Local |
| **Claude Desktop** | `active_provider` (Proveedor cloud global) | Pasarela cloud (DeepSeek, MiMo, OpenRouter, etc.) |
| **Antigravity MCP** | `mcp.provider` / `mcp.profile_id` | Proveedor dedicado de planificación y revisión MCP |

---

## Solución de problemas

### La actualización no encuentra Ollama
1. Confirme que Ollama se esté ejecutando:
   ```powershell
   ollama list
   ```
2. Compruebe que el punto de conexión sea `http://127.0.0.1:11434`.

### El modelo guardado no aparece en la lista
Anthro Bridge conserva el modelo configurado. También puede ingresarlo manualmente.

### Claude Code sigue utilizando el proveedor en la nube
1. Confirme en el panel que la tarjeta **Ollama Local** esté activa.
2. Asegúrese de haber iniciado Claude Code con el comando generado por Anthro Bridge.

### Claude Desktop sigue utilizando el proveedor en la nube
Comportamiento esperado. Ollama Local es exclusivo para Claude Code.
