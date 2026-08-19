[English](ANTIGRAVITY_MCP.md) | [日本語](ANTIGRAVITY_MCP.ja.md) | [中文(简体)](ANTIGRAVITY_MCP.zh-CN.md) | [中文(繁體)](ANTIGRAVITY_MCP.zh-TW.md) | [한국어](ANTIGRAVITY_MCP.ko.md) | [Français](ANTIGRAVITY_MCP.fr.md) | [Deutsch](ANTIGRAVITY_MCP.de.md) | [Español](ANTIGRAVITY_MCP.es.md)

[← Volver al README de Anthro Bridge](README.es.md)

# Uso de Anthro Bridge MCP con Google Antigravity

Anthro Bridge incluye un servidor Model Context Protocol (MCP) integrado que proporciona una herramienta especializada `plan` (`anthro-bridge/plan`). Esto permite a entornos de agentes como Google Antigravity delegar el diseño arquitectónico y la planificación de implementación en modelos LLM externos (p. ej., DeepSeek V4, MiMo, Kimi, MiniMax o modelos de OpenRouter), mientras realiza las modificaciones de código, comandos de terminal, compilación y pruebas con la cuota de modelo incluida en la suscripción de Antigravity.

---

## 1. Funcionamiento de este flujo de trabajo

```text
Antigravity
    ↓
Exploración del repositorio (inspección de archivos y recolección de contexto)
    ↓
anthro-bridge / plan (llamada MCP con tarea, contexto y restricciones)
    ↓
Servidor MCP Anthro Bridge
    ↓
Modelo planificador externo (configurado en la interfaz GUI)
    ↓
Plan de implementación estructurado devuelto
    ↓
Antigravity ejecuta ediciones,
compilación y pruebas mediante su suscripción
```

- **API externa**: Responsable únicamente de generar el plan de implementación basado en el contexto del repositorio (facturado por uso por el proveedor respectivo).
- **Suscripción de Antigravity**: Se encarga del trabajo intensivo de lectura/escritura de archivos, edición de código, ejecución de herramientas y bucles de prueba.
- **Separación de responsabilidades**: Aproveche la capacidad de razonamiento de los modelos externos sin agotar tokens de API en la generación de código rutinario.

---

## 2. Requisitos previos

1. **Anthro Bridge** instalado en Windows.
2. **`anthro-bridge-mcp-server.exe`** compilado o disponible en su directorio de instalación (p. ej.: `mcp-server/target/release/anthro-bridge-mcp-server.exe`).
3. Una **clave API** configurada para el proveedor que desea utilizar como planificador.
4. **Google Antigravity** instalado y en ejecución.

---

## 3. Configurar el servidor MCP en Antigravity

1. Abra Google Antigravity.
2. Vaya a:
   ```text
   Settings → Customizations → Installed MCP Servers → Open MCP Config
   ```
3. Agregue la configuración del servidor `anthro-bridge` al objeto `mcpServers`:

```json
{
  "mcpServers": {
    "anthro-bridge": {
      "command": "C:\\Users\\<USER>\\path\\to\\anthro-bridge\\mcp-server\\target\\release\\anthro-bridge-mcp-server.exe"
    }
  }
}
```

> [!TIP]
> No necesita escribir sus claves API en texto plano en la configuración MCP. El servidor MCP lee automáticamente las variables de entorno de usuario de Windows (`DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`, `MOONSHOT_API_KEY`, `MINIMAX_API_KEY`, `XIAOMI_API_KEY`, etc.) o la configuración guardada en Anthro Bridge.

---

## 4. Verificar la conexión MCP

En la vista **Installed MCP Servers** de Antigravity, confirme que `anthro-bridge` esté reconocido:

```text
anthro-bridge
  1 tool enabled
  - plan
```

---

## 5. Configurar el modelo planificador en Anthro Bridge

1. Abra la aplicación de escritorio **Anthro Bridge**.
2. Seleccione la pestaña **MCP** en la parte superior.
3. Elija el **Proveedor (Provider)** o **Perfil (Profile)** de planificador activo (DeepSeek, MiMo, OpenRouter, etc.).
4. Abra **Configuración** (o Configuración detallada del plan MCP) para configurar:
   - **Modelo (Model)**
   - **Modo Thinking**
   - **Esfuerzo de razonamiento (Reasoning Effort)**
5. Guarde la configuración.

> [!NOTE]
> El servidor MCP de Anthro Bridge lee dinámicamente la configuración actual en cada invocación de la herramienta `plan()`. **No** necesita reiniciar el servidor MCP ni Antigravity al cambiar de proveedor o modelo en la GUI.

---

## 6. Uso manual de la herramienta plan

Puede pedirle directamente a Antigravity en el chat que invoque al planificador:

```text
Inspecciona este proyecto y luego utiliza la herramienta MCP anthro-bridge/plan para crear un plan de implementación. No comiences a implementar todavía.
```

Antigravity explorará los archivos pertinentes, resumirá el contexto, llamará a `anthro-bridge/plan` y le presentará el plan resultante para su revisión.

---

## 7. Automatización de la planificación con una regla de espacio de trabajo

Cree un archivo de regla de espacio de trabajo en [`.agents/rules/deepseek-planner.md`](../.agents/rules/deepseek-planner.md) para automatizar la invocación del planificador en tareas de código complejas:

```markdown
---
trigger: model_decision
description: Use for implementation, debugging, architectural changes, or multi-file code changes where an external implementation plan would be useful. Do not use for trivial text-only edits.
---

# Planner Rule

For non-trivial implementation tasks in this repository:

1. First inspect the repository yourself and identify the files and code relevant to the task.
2. Summarize only the context necessary for implementation planning.
3. Call the `anthro-bridge/plan` MCP tool exactly once with:
   - the user's task;
   - the relevant repository context;
   - important constraints.
4. Use the returned plan as the basis for implementation.
5. Perform file edits, builds, and tests yourself.
6. Do not call `anthro-bridge/plan` again unless the implementation encounters a major unresolved problem.
7. Do not ask the user to repeat this workflow.
8. Do not use the planner for trivial tasks such as a one-word text change unless planning would materially help.
```

---

## 8. Flujo de trabajo automatizado típico

```text
Usuario: "Refactoriza la función X para admitir múltiples perfiles."
    ↓
Antigravity inspecciona el código y resume el contexto
    ↓
Antigravity activa automáticamente la llamada a anthro-bridge/plan
    ↓
Anthro Bridge envía la solicitud al modelo externo seleccionado
    ↓
Antigravity recibe el plan de implementación estructurado
    ↓
El usuario revisa y aprueba el plan
    ↓
Antigravity realiza los cambios en los archivos y ejecuta las pruebas
```

---

## 9. Notas importantes

- **Operación independiente**: El servidor MCP opera independientemente de la pasarela 3P Gateway de Anthro Bridge. La pasarela 3P no necesita estar en ejecución para que funcionen las llamadas MCP.
- **Facturación separada**: Las llamadas a `anthro-bridge/plan` generan costos de API facturados por el proveedor externo seleccionado. Las ediciones posteriores y las pruebas utilizan la cuota de suscripción de Antigravity.
- **Efecto inmediato**: Cambiar la configuración del planificador en la GUI de Anthro Bridge surte efecto de inmediato en la siguiente invocación de `plan()`.
