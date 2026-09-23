[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

**Usa Claude Code / Claude Desktop como entorno de desarrollo, enruta la inferencia a APIs de LLM de terceros y utiliza modelos externos como planificadores y revisores para Google Antigravity.**

Anthro Bridge es una aplicación complementaria para Windows orientada al desarrollo de software asistido por IA. Admite dos flujos de trabajo complementarios:

1. **Gateway 3P para Claude Code / Claude Desktop** — Conserva la exploración de repositorios, el uso de herramientas, la edición de archivos y la ejecución de pruebas de Claude mientras enruta la inferencia a proveedores de terceros.
2. **Planificador y Revisor MCP para Google Antigravity** — Delega la planificación de implementaciones y la revisión post-implementación a modelos externos mediante las herramientas MCP `anthro-bridge/plan` y `anthro-bridge/review`.

---

## Dos Flujos de Trabajo Principales

### 1. Claude Code / Claude Desktop con Gateway 3P

```text
Claude Code / Claude Desktop
             ↓
  Anthro Bridge 3P Gateway
             ↓
DeepSeek / Kimi Code / OpenRouter / MiniMax / MiMo
```

- **Separación de Entorno y Modelo**: Conserva las herramientas agénticas de Claude mientras enruta la inferencia a proveedores de terceros.
- **Enrutamiento Dinámico Multi-Perfil**: Cambia proveedores activos, perfiles de OpenRouter y rutas de modelos desde la interfaz gráfica.
- **Guía de Configuración**: [Configuración del Gateway 3P para Claude Desktop / Cowork](THIRD_PARTY_INFERENCE.es.md)

### 2. Antigravity con Planificador y Revisor MCP

```text
Antigravity
    ↓ stdio
anthro-bridge.exe --mcp-server
    ↓
Modelo externo configurado (planificador / revisor)
    ↓
Plan de implementación / Veredicto de revisión
    ↓
Antigravity implementa y prueba
usando capacidad respaldada por suscripción
```

- **División Planificación / Ejecución**: Los modelos externos generan el plan de alto nivel o el veredicto de revisión; la capacidad de suscripción de Antigravity ejecuta las ediciones de código intensivas en tokens.
- **Configuración en Vivo desde la GUI**: Cambiar el proveedor, el modelo o el nivel de razonamiento del planificador o revisor tiene efecto inmediato en la siguiente invocación.
- **Guía de Configuración**: [Configuración de Google Antigravity + Anthro Bridge MCP](ANTIGRAVITY_MCP.es.md)

**Comandos globales de Antigravity:**

- **`/anthro-plan`** — Delega la planificación de la implementación al modelo externo configurado.
- **`/anthro-revise`** — Revisa un plan existente en función de nuevos comentarios o restricciones.
- **`/anthro-review`** — Revisa una implementación completada contra el plan aprobado antes de hacer commit, con veredictos explícitos READY / NOT READY.

**Flujo de trabajo recomendado:**

```text
/anthro-plan → Implementación y Pruebas → /anthro-review → Commit
```

---

## Proveedores Compatibles

| Proveedor | Conexión | Familias Compatibles | Controles de Razonamiento |
|---|---|---|---|
| **DeepSeek** | API Directa | DeepSeek V4.1 Flash, V4 Pro 0813 | Normal / Low / High / Max |
| **Kimi Code** | API Directa | kimi-for-coding, kimi-for-coding-highspeed | Modo de pensamiento |
| **MiniMax** | API Directa | MiniMax M3, M2.7 | Específico del modelo |
| **Kimi / Moonshot** | API Directa | Kimi K2.x, Kimi K3 | Thinking / Reasoning effort |
| **MiMo / Xiaomi** | API Directa | MiMo V2.6 Flash, Pro, Pro-UltraSpeed (compatible con V2.5) | Normal / Thinking |
| **OpenRouter** | Gateway Multi-Perfil | Ver la sección de OpenRouter a continuación | Específico del modelo / del perfil |

### DeepSeek (Directo)

El preset integrado **Direct DeepSeek** enruta: Opus 5 → V4.1 Flash / Max · Sonnet 5 → V4.1 Flash / High · Haiku 4.5 → V4.1 Flash / Low.

- `deepseek-v4.1-flash` — Razonador principal actual ($0.27 / 1M de entrada · $1.10 / 1M de salida).
- `deepseek-v4-pro-0813` — Línea de base de alta calidad sin razonamiento extendido ($0.27 / 1M de entrada · $1.10 / 1M de salida).

### Kimi Code (Directo)

API especializada en codificación (`KIMI_CODE_API_KEY`), independiente de Moonshot Kimi:

- `kimi-for-coding` — Modelo de codificación de calidad completa.
- `kimi-for-coding-highspeed` — Variante de baja latencia.

### MiMo / Xiaomi (Directo)

Presets integrados de **Direct MiMo**:
- Opus 5 → `mimo-v2.6-pro` / Thinking
- Sonnet 5 → `mimo-v2.6-pro` / Normal
- Haiku 4.5 → `mimo-v2.6-flash` / Thinking
- Modelo predeterminado: `mimo-v2.6-flash`

Modelos: `mimo-v2.6-flash`, `mimo-v2.6-pro`, `mimo-v2.6-pro-ultraspeed` (seleccionables). Los tres admiten una ventana de contexto de 1 millón de tokens y capacidades multimodales nativas (texto, imagen, vídeo).

**Normal / Thinking**: MiMo usa un simple interruptor Normal/Thinking — sin niveles de esfuerzo de razonamiento.

**Compatibilidad con V2.5**: Las rutas guardadas `mimo-v2.5`, `mimo-v2.5-pro` y `mimo-v2.5-pro-ultraspeed` se conservan y siguen funcionando. Los valores predeterminados heredados sin modificar migran automáticamente a V2.6 al inicio.

### OpenRouter

Admite múltiples perfiles con nombre. Catálogo completo de modelos OpenAI (un único desplegable):

| ID del Modelo | Nombre Mostrado |
|---|---|
| `openai/gpt-6-astra` | GPT-6 Astra |
| `openai/gpt-6-astra-pro` | GPT-6 Astra Pro |
| `openai/gpt-astra-latest` | GPT Astra Latest |
| `openai/gpt-5.6-sol` | GPT-5.6 Sol |
| `openai/gpt-5.6-sol-pro` | GPT-5.6 Sol Pro |
| `openai/gpt-5.6-terra` | GPT-5.6 Terra |
| `openai/gpt-5.6-terra-pro` | GPT-5.6 Terra Pro |
| `openai/gpt-5.6-luna` | GPT-5.6 Luna |
| `openai/gpt-5.6-luna-pro` | GPT-5.6 Luna Pro |

**GPT-6 Astra**: 1.05M de contexto · nivel de razonamiento: `low / medium / high / xhigh / max`.  
**GPT-6 Astra Pro**: 1.05M de contexto · razonamiento Pro siempre activo (`reasoning.mode = pro`), sin selección de nivel por parte del usuario.  
**GPT Astra Latest**: alias que sigue el modelo más reciente de la familia Astra.

Preset integrado **OpenRouter: chatGPT**: Opus 5 → GPT-6 Astra / max · Sonnet 5 → GPT-6 Astra / high · Haiku 4.5 → GPT-6 Astra / medium.

También disponibles: **OpenRouter: Gemini** (Gemini 3.8 Flash · nivel de razonamiento `low / medium / high`), **OpenRouter: Poolside**, **OpenRouter: Tencent**, **OpenRouter: InclusionAI**, **OpenRouter: StepFun**.

---

## Precios de Modelos (a partir de v0.22.1)

| Modelo | Entrada | Salida |
|---|---|---|
| DeepSeek V4.1 Flash | \$0.27 / 1M | \$1.10 / 1M |
| DeepSeek V4 Pro 0813 | \$0.27 / 1M | \$1.10 / 1M |
| GPT-6 Astra / Astra Pro / Astra Latest | \$10 / 1M | \$50 / 1M |
| GPT-5.6 Sol / Terra / Luna | \$5 / 1M | \$25 / 1M |
| GPT-5.6 Sol Pro / Terra Pro / Luna Pro | \$5 / 1M | \$25 / 1M |
| Gemini 3.8 Flash (OpenRouter) | \$0.75 / 1M | \$3.75 / 1M |
| MiMo-V2.6-Flash | \$0.14 / 1M | \$0.28 / 1M |
| MiMo-V2.6-Pro | \$0.435 / 1M | \$0.87 / 1M |
| MiMo-V2.6-Pro-UltraSpeed | \$4.35 / 1M | \$8.70 / 1M |

---

## Instalación

Descarga el instalador más reciente para Windows (`Anthro Bridge_x.x.x_x64-setup.exe`) desde la página de [Releases](https://github.com/soheidon/anthro-bridge/releases) y ejecútalo.

El instalador es compatible con 8 idiomas y conserva la configuración de usuario existente durante las actualizaciones.

---

## Inicio Rápido

### Flujo de Trabajo 1: Gateway 3P para Claude Code / Claude Desktop

1. Abre Anthro Bridge en **Settings > API Key** y configura una clave de API para el proveedor deseado.
2. Selecciona tu proveedor o perfil de OpenRouter en el panel principal.
3. Haz clic en **Start Gateway** (se ejecuta en `http://127.0.0.1:4000`).
4. Conecta Claude Code o Claude Desktop:
   - **Claude Code**: Haz clic en **Copy Claude Code launch command** en Settings y pégalo en PowerShell.
   - **Claude Desktop / Cowork**: Sigue la [Guía de Configuración 3P para Claude Desktop](THIRD_PARTY_INFERENCE.es.md).

### Flujo de Trabajo 2: Planificador y Revisor MCP para Google Antigravity

1. Configura una clave de API para el modelo planificador/revisor elegido en Anthro Bridge.
2. Selecciona la pestaña **MCP** y configura tu modelo en **Settings > Antigravity > MCP Plan Settings**.
3. Registra `anthro-bridge.exe` con `["--mcp-server"]` en la configuración MCP de Antigravity (o haz clic en **Configure Automatically** en Anthro Bridge).
4. Usa `/anthro-plan` para diseñar planes, `/anthro-revise` para actualizar planes y `/anthro-review` para revisar implementaciones antes de hacer commit.
5. Sigue la [Guía Completa de Configuración de MCP para Antigravity](ANTIGRAVITY_MCP.es.md).

---

## Claves de API

| Proveedor | Variable de Entorno |
|---|---|
| DeepSeek | `DEEPSEEK_API_KEY` |
| Kimi Code | `KIMI_CODE_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| MiMo / Xiaomi | `XIAOMI_API_KEY` |
| OpenRouter | `OPENROUTER_API_KEY` |

---

## Documentación

- [Configuración del Gateway 3P para Claude Desktop / Cowork](THIRD_PARTY_INFERENCE.es.md)
- [Configuración de Google Antigravity + Anthro Bridge MCP](ANTIGRAVITY_MCP.es.md)
- [Referencia de Configuración (`config.json`)](CONFIGURATION.md)
- [Detalles de Proveedores y Controles de Razonamiento](PROVIDERS.md)
- [Guía de Desarrollo y Verificación](DEVELOPMENT.md)

---

## Solución de Problemas

### El Puerto 4000 Está Ocupado
```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### La Configuración Se Revierte Después de una Actualización
Reinicia la aplicación para que las migraciones puedan ejecutarse. La configuración se almacena en `%APPDATA%\Anthro Bridge\config.json`.

### Las Llamadas al Planificador MCP Fallan
Asegúrate de que haya una clave de API configurada para el proveedor seleccionado en la pestaña **MCP**, o que esté exportada en las variables de entorno de usuario de Windows (por ejemplo, `DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`). El Gateway 3P no necesita estar en ejecución para usar MCP.

---

## Licencia

Licencia MIT. Ver [LICENSE](../LICENSE).
