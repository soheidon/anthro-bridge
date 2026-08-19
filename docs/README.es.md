[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

**Use Claude Code Desktop como entorno de desarrollo, enrute la implementación a API de terceros y utilice modelos externos como planificadores para Antigravity.**

Anthro Bridge es una aplicación complementaria para Windows para el desarrollo de software asistido por IA, estructurada en torno a dos flujos de trabajo principales:

1. **Claude Code / Claude Desktop + Pasarela de terceros (3P Gateway)**: Continúe usando Claude Code Desktop como entorno de desarrollo de agentes mientras enruta las solicitudes a través de una pasarela local compatible con Anthropic hacia API de LLM de terceros (DeepSeek, MiMo, MiniMax, Kimi y OpenRouter).
2. **Antigravity + Planificador MCP (MCP Planner)**: Delegue el diseño arquitectónico y la planificación de implementación a modelos externos mediante la herramienta `plan` MCP de Anthro Bridge (`anthro-bridge/plan`), mientras realiza las ediciones de código y pruebas con la cuota de modelo incluida en su suscripción de Antigravity.

---

## Dos flujos de trabajo principales

### 1. Claude Code / Claude Desktop con 3P Gateway

Siga utilizando Claude Code Desktop y Claude Desktop como entorno de desarrollo mientras enruta solicitudes a API de LLM de terceros no admitidas nativamente por los clientes de Anthropic.

```text
Claude Code / Claude Desktop
             ↓
  Pasarela 3P Anthro Bridge
             ↓
DeepSeek / MiniMax / Kimi / MiMo / OpenRouter
```

- **Separación de entorno y modelo**: Conserve la exploración de repositorios, el uso de herramientas, la edición de archivos y las pruebas de Claude mientras enruta la inferencia a proveedores de terceros.
- **Enrutamiento dinámico multifolders**: Cambie de proveedor activo o perfil de OpenRouter al instante desde el panel GUI y personalice las rutas de Opus, Sonnet y Haiku en la configuración.
- **Guía de configuración**: [Guía de configuración de 3P Gateway para Claude Desktop](THIRD_PARTY_INFERENCE.es.md)

### 2. Antigravity con Planificador MCP

Delegue la planificación de implementación y el diseño a modelos externos mediante la herramienta `plan` MCP de Anthro Bridge (`anthro-bridge/plan`), mientras ejecuta las modificaciones de código y comandos de terminal con la capacidad de suscripción de Antigravity.

```text
Antigravity
    ↓
Exploración del repositorio (recolección de contexto)
    ↓
anthro-bridge / plan (MCP)
    ↓
Servidor MCP Anthro Bridge
    ↓
Modelo LLM externo configurado
    ↓
Plan de implementación estructurado devuelto
    ↓
Antigravity ejecuta ediciones,
compilación y pruebas mediante su suscripción
```

- **División de planificación y ejecución**: Los modelos externos generan el plan de alto nivel; la suscripción de Antigravity ejecuta las modificaciones de código y las pruebas que consumen muchos tokens.
- **Configuración GUI en tiempo real**: Cambiar el proveedor, modelo o intensidad de razonamiento en Anthro Bridge surte efecto de inmediato en la siguiente llamada a `plan()`, sin reiniciar Antigravity.
- **Guía de configuración**: [Guía de configuración de Google Antigravity + MCP Anthro Bridge](ANTIGRAVITY_MCP.es.md)

---

## Proveedores compatibles

| Proveedor | Tipo de conexión | Familias de modelos compatibles | Controles de razonamiento |
|---|---|---|---|
| **DeepSeek** | API directa | DeepSeek V4 Pro, V4 Flash | Normal / Low / High / Max |
| **MiniMax** | API directa | MiniMax M3, M2.7 | Específico del modelo |
| **Kimi / Moonshot** | API directa | Kimi K2.x, Kimi K3 | Thinking / Esfuerzo de razonamiento |
| **MiMo / Xiaomi** | API directa | MiMo V2.5, V2.5 Pro | Modo Thinking |
| **OpenRouter** | Pasarela multiperfil | Poolside, Tencent, InclusionAI, StepFun, OpenAI GPT-5.6, Google Gemini, etc. | Específico del modelo / perfil |

---

## Instalación

Descargue el instalador de Windows más reciente (`Anthro Bridge_x.x.x_x64-setup.exe`) desde la página de [Releases](https://github.com/soheidon/anthro-bridge/releases) y ejecútelo.

El instalador admite 8 idiomas (inglés, japonés, chino simplificado, chino tradicional, coreano, francés, alemán, español) y conserva las configuraciones de usuario existentes durante las actualizaciones.

---

## Inicio rápido

### Flujo 1: Pasarela 3P para Claude Code / Claude Desktop

1. Abra **Configuración > Clave API** en Anthro Bridge y configure una clave para el proveedor deseado.
2. Seleccione su proveedor o perfil de OpenRouter en el panel de control.
3. Haga clic en **Iniciar pasarela (Start Gateway)** (escucha en `http://127.0.0.1:4000`).
4. Conecte Claude Code o Claude Desktop:
   - **Claude Code**: Haga clic en **Copiar comando de inicio de Claude Code** en la configuración y péguelo en PowerShell.
   - **Claude Desktop / Cowork**: Siga la [Guía de configuración 3P para Claude Desktop](THIRD_PARTY_INFERENCE.es.md).

### Flujo 2: Planificador MCP para Google Antigravity

1. Configure una clave API para el modelo de planificador elegido en Anthro Bridge.
2. Seleccione la pestaña **MCP** en Anthro Bridge y configure su modelo en **Configuración > Configuración detallada del plan MCP**.
3. Registre `anthro-bridge-mcp-server.exe` en la configuración MCP de Antigravity.
4. Llame a `anthro-bridge/plan` en Antigravity (o automatícelo con una regla de espacio de trabajo).
5. Siga la [Guía completa de configuración MCP para Antigravity](ANTIGRAVITY_MCP.es.md).

---

## Documentación

- [Guía de configuración 3P Gateway para Claude Desktop](THIRD_PARTY_INFERENCE.es.md)
- [Guía de configuración Google Antigravity + MCP Anthro Bridge](ANTIGRAVITY_MCP.es.md)
- [Referencia de configuración (`config.json`)](CONFIGURATION.md)
- [Detalles de proveedores y comportamiento de modelos](PROVIDERS.md)
- [Guía de desarrollo y verificación](DEVELOPMENT.md)

---

## Solución de problemas

### El puerto 4000 ya está en uso
```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### La configuración se restablece tras una actualización
Reinicie la aplicación para que se ejecuten las migraciones. La configuración se guarda en `%APPDATA%\Anthro Bridge\config.json`.

### Fallo al llamar al planificador MCP
Asegúrese de que haya una clave API configurada para el proveedor seleccionado en la pestaña **MCP** de Anthro Bridge o en las variables de entorno de usuario de Windows (`DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`, etc.). No es necesario que la pasarela 3P esté en ejecución para usar MCP.

---

## Licencia

Licencia MIT. Consulte [LICENSE](../LICENSE).
