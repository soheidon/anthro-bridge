[English](ANTIGRAVITY_MCP.md) | [日本語](ANTIGRAVITY_MCP.ja.md) | [中文(简体)](ANTIGRAVITY_MCP.zh-CN.md) | [中文(繁體)](ANTIGRAVITY_MCP.zh-TW.md) | [한국어](ANTIGRAVITY_MCP.ko.md) | [Français](ANTIGRAVITY_MCP.fr.md) | [Deutsch](ANTIGRAVITY_MCP.de.md) | [Español](ANTIGRAVITY_MCP.es.md)

[← Volver al README de Anthro Bridge](README.es.md)

# Uso de Anthro Bridge MCP con Google Antigravity

Anthro Bridge no requiere un ejecutable de servidor MCP independiente. El único archivo `anthro-bridge.exe` instalado proporciona tanto la aplicación GUI de escritorio como las funciones de servidor MCP. Antigravity inicia el modo MCP ejecutando ese mismo archivo con el argumento `--mcp-server`.

```text
Inicio normal
anthro-bridge.exe
→ Aplicación de escritorio Anthro Bridge / 3P Gateway

Inicio MCP
anthro-bridge.exe --mcp-server
→ Servidor MCP stdio headless para Antigravity
```

Esto permite a entornos de agentes como Google Antigravity delegar el diseño arquitectónico y la planificación de implementación en modelos LLM externos (p. ej., DeepSeek V4, MiMo, Kimi, MiniMax o modelos de OpenRouter) a través de `anthro-bridge/plan`, mientras realiza las modificaciones de código, comandos de terminal, compilación y pruebas de alto consumo de tokens con la cuota incluida en la suscripción de Antigravity.

---

## 1. Funcionamiento de este flujo de trabajo

```text
Antigravity
    ↓ stdio
anthro-bridge.exe --mcp-server
    ↓
Modelo planificador externo configurado
    ↓
Plan de implementación estructurado devuelto
    ↓
Antigravity ejecuta ediciones,
compilación y pruebas mediante su suscripción
```

---

## 2. Requisitos previos

1. **Anthro Bridge** instalado en Windows.
2. Autenticación del proveedor configurada en Anthro Bridge o en las variables de entorno del sistema para el planificador que desea utilizar.
3. **Google Antigravity** instalado y en ejecución.

---

## 3. Configurar el servidor MCP en Antigravity

### Método 1 — Configuración mediante la GUI de Anthro Bridge (Recomendada)

1. Abra Anthro Bridge y vaya a **Configuración** (pestaña `[Configuración]`) > subnavegación izquierda **Antigravity**.
2. Revise la tarjeta **Integración con Google Antigravity**:
   - **Ejecutable de destino**: Muestra por defecto la ruta de `anthro-bridge.exe` en ejecución. Para usar otro binario (portable o desarrollo propio), haga clic en **Cambiar** (`antigravity.btnChangeExe`) y seleccione el ejecutable.
   - **Registrar / Actualizar**: Haga clic en **Actualizar configuración de Antigravity** (`antigravity.btnUpdate`) para registrar o actualizar de forma segura `anthro-bridge` en `%USERPROFILE%\.gemini\config\mcp_config.json`, preservando intactos los demás servidores MCP.
   - **Eliminar**: Haga clic en **Eliminar configuración** (`antigravity.btnRemove`) si desea anular el registro del servidor en Antigravity.
   - **Abrir carpeta**: Haga clic en **Abrir carpeta de configuración** (`antigravity.btnOpenFolder`) para inspeccionar el directorio en el Explorador de Windows.

---

### Método 2 — Configuración manual (Avanzada)

1. En Anthro Bridge **Configuración > Antigravity**, haga clic en **Abrir carpeta de configuración** para abrir `%USERPROFILE%\.gemini\config\` en el Explorador de Windows.
2. Abra o cree `mcp_config.json` y agregue `anthro-bridge` dentro de `mcpServers`:

```json
{
  "mcpServers": {
    "anthro-bridge": {
      "command": "C:\\Users\\<USER>\\AppData\\Local\\Anthro Bridge\\anthro-bridge.exe",
      "args": ["--mcp-server"]
    }
  }
}
```

Para compilaciones de desarrollo, apunte directamente al ejecutable de Release:
```json
{
  "mcpServers": {
    "anthro-bridge": {
      "command": "C:\\Users\\<USER>\\path\\to\\anthro-bridge\\gui\\src-tauri\\target\\release\\anthro-bridge.exe",
      "args": ["--mcp-server"]
    }
  }
}
```

> [!TIP]
> **No** necesita escribir claves API en el archivo `mcp_config.json` de Antigravity. El servidor MCP utiliza el mecanismo de resolución de credenciales de Anthro Bridge (lectura automática de variables de entorno de Windows como `DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`, `MOONSHOT_API_KEY`, `MINIMAX_API_KEY`, `XIAOMI_API_KEY` o configuración guardada).

---

## 4. Verificar la conexión MCP

En la vista **Installed MCP Servers** de Antigravity, confirme que `anthro-bridge` esté reconocido:

```text
anthro-bridge
  1 tool enabled
  - plan
```

---

## 5. Configurar los modelos planificadores en Anthro Bridge

Anthro Bridge separa claramente la selección del planificador de la gestión detallada de parámetros:

1. **Pestaña superior `MCP` (`MCP for Antigravity`)**:
   - Muestra tarjetas de los proveedores disponibles (DeepSeek, OpenRouter, MiniMax, MiMo, Kimi) y perfiles.
   - Haga clic en una tarjeta para cambiar de inmediato el destino del planificador activo.
2. **`Configuración` > `Antigravity`**:
   - Tarjeta **Configuración detallada de MCP**: Configure el modelo, el modo Thinking y el nivel Reasoning Effort por proveedor/perfil.
   - Tarjeta **Integración con Google Antigravity**: Gestione el registro del servidor MCP y los comandos de Antigravity (Global Skills).

> [!NOTE]
> El servidor MCP de Anthro Bridge lee dinámicamente la configuración actual en cada invocación de la herramienta `plan()`. **No** necesita reiniciar el servidor MCP ni Antigravity al cambiar de proveedor o parámetros en la GUI.

---

## 6. Uso de los comandos de Antigravity (`/anthro-plan` y `/anthro-revise`) (Recomendado)

Desde **Configuración > Antigravity > Integración con Google Antigravity**, instale habilidades globales para utilizar comandos de barra en todos sus espacios de trabajo de Antigravity:

- Haga clic en **Instalar todos** (`antigravity.btnInstallAll`) o en **Instalar** (`antigravity.commandBtnInstall`) junto a cada comando.

### Crear un nuevo plan de implementación:
```text
/anthro-plan <descripción de la tarea o función a implementar>
```
*Recopila el contexto del repositorio, invoca `anthro-bridge/plan` y se detiene de forma limpia tras presentar el plan, sin modificar archivos ni compilar.*

### Revisar un plan de implementación existente:
```text
/anthro-revise <comentarios o nuevos requisitos a incorporar>
```
*Identifica el plan actual (contexto activo o `implementation_plan.md`), pasa el plan y los comentarios a `anthro-bridge/plan`, y actualiza el plan preservando las secciones no afectadas.*

> [!IMPORTANT]
> Al ejecutar mediante `/anthro-plan` o `/anthro-revise`, el propio comando gestiona la única llamada al planificador. Las reglas de Workspace no generarán llamadas adicionales duplicadas.

---

## 7. Automatización de la planificación con Workspace Rules

Coloque una regla de espacio de trabajo como [`.agents/rules/deepseek-planner.md`](../.agents/rules/deepseek-planner.md) en su proyecto para automatizar la invocación del planificador en tareas complejas:

```markdown
---
trigger: model_decision
description: Use for implementation, debugging, architectural changes, or multi-file code changes where an external implementation plan would be useful. Do not use for trivial text-only edits.
---

# DeepSeek Planner Rule

For non-trivial implementation tasks in this repository:

1. If the current task is being executed through the `/anthro-plan` or `/anthro-revise` command, do NOT invoke `anthro-bridge/plan` separately. The command workflow owns the planner call.
2. First inspect the repository yourself and identify the files and code relevant to the task.
3. Summarize only the context necessary for implementation planning.
4. Call the `anthro-bridge/plan` MCP tool exactly once with:
   - the user's task;
   - the relevant repository context;
   - important constraints.
   Note: "Exactly once" means duplicate planner calls are prohibited once a successful usable result is obtained. If the tool call itself fails or returns an unusable response (e.g. transport or decoding error), exactly 1 recovery retry is permitted.
5. Use the returned DeepSeek plan as the basis for implementation.
6. Perform file edits, builds, and tests yourself.
7. Do not call `anthro-bridge/plan` again unless the implementation encounters a major unresolved problem.
8. Do not ask the user to repeat this workflow.
9. Do not use the planner for trivial tasks such as a one-word text change unless planning would materially help.
```

### Política de activación:
- **Tareas triviales / localizadas (Trivial / localized tasks)** (p. ej., corregir erratas, cambios de una línea, ajustes de sintaxis simples): No activan el planificador.
- **Tareas complejas (Non-trivial tasks)** (cambios arquitectónicos, funciones multi-archivo, depuración compleja): Antigravity inspecciona el repositorio, invoca `anthro-bridge/plan` 1 sola vez y realiza la implementación según el plan obtenido.

---

## 8. Flujo de trabajo automatizado típico

```text
Usuario: "Refactoriza la función X para admitir múltiples perfiles."
    ↓
Antigravity inspecciona el código y resume el contexto
    ↓
Antigravity activa automáticamente la llamada a anthro-bridge/plan (1 sola vez)
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

- **Operación independiente**: El servidor MCP opera de forma completamente independiente de la pasarela 3P Gateway. La pasarela 3P no necesita estar encendida (ON) para que funcionen las llamadas MCP.
- **Facturación separada**: Las llamadas a `anthro-bridge/plan` generan costos de API facturados por el proveedor externo seleccionado. Las ediciones posteriores y las pruebas utilizan la cuota de suscripción de Antigravity.
- **Efecto inmediato**: Cambiar proveedores o parámetros de modelo en la GUI de Anthro Bridge surte efecto de inmediato en la siguiente invocación de `plan()`.
