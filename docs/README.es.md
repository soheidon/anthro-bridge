[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

## Resumen

Anthro Bridge es una herramienta de proxy + gestión con GUI que enruta solicitudes API de Claude Desktop / Claude Code a través de múltiples proveedores con endpoints compatibles con Anthropic.

Anthro Bridge lee el campo `model` de cada solicitud y la enruta automáticamente al proveedor upstream correcto (enrutamiento basado en modelos). Solo se reescribe el campo `model` — los mensajes, bloques thinking, tool_use, tool_result y streaming SSE pasan sin modificaciones.

Anthro Bridge no es un fork, GUI o aplicación complementaria de Moon Bridge; es una puerta de enlace independiente compatible con Anthropic.

### Proveedores soportados

| ID del proveedor | Nombre para mostrar | Endpoint upstream | Modelo predeterminado |
|------------------|--------------------|-------------------|-----------------------|
| `deepseek` | DeepSeek | `https://api.deepseek.com/anthropic` | `deepseek-v4-pro` |
| `minimax` | MiniMax | `https://api.minimax.io/anthropic` | `MiniMax-M3` |
| `kimi` | Kimi / Moonshot | `https://api.moonshot.cn/anthropic` | `kimi-k2.7-code` |
| `mimo` | **MiMo / Xiaomi** | `https://api.xiaomimimo.com/anthropic` | `mimo-v2.5-pro` |

La herramienta de gestión con GUI (Tauri v2 + React 19 + TypeScript) ofrece control de inicio/detención, edición de configuración, visualización de registros y gestión de API keys desde una ventana nativa de Windows.

### Por qué se necesita esta puerta de enlace

Claude Desktop / Claude Code espera fundamentalmente el formato de API y los nombres de modelo de la familia Claude de Anthropic. Incluso cuando proveedores como DeepSeek, MiniMax, Kimi y MiMo ofrecen APIs compatibles con Anthropic, Claude Desktop / Claude Code no siempre puede usarlas directamente.

En particular, **`inferenceModels[].name` de Claude Desktop solo acepta nombres de modelo oficiales de Anthropic**. Los nombres personalizados del gateway como `claude-deepseek-v4` o `kimi-k2.6` son rechazados con `"not an Anthropic model"`.

Para sortear esta limitación, Anthro Bridge **presenta nombres de modelo oficiales de Anthropic (`claude-sonnet-4-6` / `claude-haiku-4-5`) como "carcasas" para Claude Desktop, mientras el LLM real (DeepSeek / MiniMax / Kimi / MiMo) se selecciona en la GUI**.

```
Lado de Claude Desktop (siempre fijo)
  Sonnet 4.6   = claude-sonnet-4-6
  Haiku 4.5 = claude-haiku-4-5

Gateway interno (basado en la selección de GUI)
  DeepSeek:      Sonnet -> deepseek-v4-pro,     Haiku -> deepseek-v4-flash
  MiniMax:       Sonnet -> MiniMax-M3,           Haiku -> MiniMax-M3
  Kimi:          Sonnet -> kimi-k2.7-code,      Haiku -> kimi-k2.6 (thinking disabled)
  MiMo / Xiaomi: Sonnet -> mimo-v2.5-pro,      Haiku -> mimo-v2.5
```

Esto le permite pasar la validación de nombres de modelo de Claude Desktop mientras cambia libremente entre DeepSeek, MiniMax, Kimi y MiMo.

### Prerrequisitos

- **Windows 10/11** (localización japonesa soportada)
- API key del proveedor elegido (DeepSeek / MiniMax / Kimi / MiMo — **uno solo es suficiente**, desde v0.5.0)

### Inicio rápido

#### 1. Instalación

Descargue el instalador más reciente desde [Releases](https://github.com/soheidon/anthro-bridge/releases) y ejecútelo.

El instalador muestra una selección de idioma al iniciar (elige entre English, 日本語, 中文(简体), 中文(繁體), 한국어, Français, Deutsch, Español).

#### Actualización

Simplemente ejecute el nuevo `setup.exe` — detecta y reemplaza automáticamente la versión anterior. No se requiere desinstalación manual. Su configuración (`%APPDATA%\Anthro Bridge\config.json`) se conserva durante las actualizaciones.

#### 2. Configurar API Key

Configuración (⚙) -> pestaña **API Key**, ingrese la API key de su proveedor y haga clic en **Guardar**.
La clave se persiste como variable de entorno de usuario de Windows.

| Proveedor | Variable de entorno | Notas |
|-----------|---------------------|-------|
| DeepSeek | `DEEPSEEK_API_KEY` | |
| MiniMax | `MINIMAX_API_KEY` | |
| Kimi / Moonshot | `MOONSHOT_API_KEY` | |
| MiMo / Xiaomi | `XIAOMI_API_KEY` | |

#### 3. Seleccionar proveedor

En el panel principal, los mosaicos de proveedores están dispuestos en una cuadrícula 2×2:

```
[ DeepSeek       ] [ MiMo / Xiaomi  ]
[ MiniMax        ] [ Kimi / Moonshot]
```

Haga clic en un mosaico para seleccionar el proveedor activo en **Seleccionar proveedor LLM**.

#### 4. Iniciar la pasarela

Haga clic en **Start Gateway** en el encabezado. El proxy se inicia en `http://127.0.0.1:4000`.

#### 5. Configurar Claude Desktop / Cowork on 3P

Consulte [docs/THIRD_PARTY_INFERENCE.md](docs/THIRD_PARTY_INFERENCE.md) para instrucciones detalladas paso a paso.

### Endpoints

| Método | Ruta | Descripción |
|--------|------|-------------|
| GET | `/health` | Verificación de salud |
| GET | `/v1/models` | Lista pública de modelos |
| POST | `/v1/messages` | Messages API (stream + non-stream). Enrutamiento basado en modelo |
| POST | `/v1/messages/count_tokens` | Conteo de tokens (solo proveedores compatibles) |

### Enrutamiento

Enrutamiento basado en modelo: el campo `model` en cada solicitud determina el proveedor de destino y el modelo upstream.

| Modelo Anthropic | DeepSeek | MiniMax | Kimi | MiMo / Xiaomi |
|------------------|----------|---------|------|---------------|
| `claude-sonnet-4-6` | `deepseek-v4-pro` | `MiniMax-M3` | `kimi-k2.7-code` | `mimo-v2.5-pro` (Thinking activado) |
| `claude-haiku-4-5` | `deepseek-v4-flash` | `MiniMax-M3` | `kimi-k2.6` (Thinking desactivado) | `mimo-v2.5` |

#### Detalles del enrutamiento MiMo

- **`claude-sonnet-4-6` → `mimo-v2.5-pro`**: Thinking está **habilitado por defecto** (`thinking_mode: "thinking"`). La clave `thinking_mode` (no `thinking`) controla el comportamiento de thinking de MiMo. Establezca en `"default"` para el modo estándar.
- **`claude-haiku-4-5` → `mimo-v2.5`**: Soporta pass-through de imágenes (URL de imagen y base64). La entrada de audio/video no es soportada por Anthro Bridge en MiMo.
- **La ruta `claude-sonnet-4-6` NO soporta imágenes.** Cuando se envían imágenes a esta ruta, se reemplazan con texto de marcador (`non_vision_image_policy: "replace"`).
- **Endpoint upstream**: Las solicitudes se envían a `https://api.xiaomimimo.com/anthropic/v1/messages`.

### Idiomas

8 idiomas: English, 日本語, 中文(简体), 中文(繁體), 한국어, Français, Deutsch, Español.

Para agregar una nueva traducción, coloque un archivo de idioma (ej., `es.ts`) en `gui/src/i18n/lang/` y reconstruya.
Consulte [CONTRIBUTING](CONTRIBUTING.md) para detalles.

### Configuración (config.json)

La configuración del proveedor define los nombres de modelo upstream y las banderas de capacidades por modelo. Normalmente no se requiere edición.
Los usuarios avanzados pueden editar en Configuración (⚙) -> **Configuración de la pasarela**.

| Clave | Descripción |
|-------|-------------|
| `models.<model>.upstream_model` | Nombre real del modelo enviado al upstream (requerido) |
| `models.<model>.thinking` | Cuando es `"disabled"`, inyecta supresión de thinking (opcional). Para MiMo, use `thinking_mode` en su lugar |
| `models.<model>.thinking_mode` | Específico de MiMo: `"thinking"` (habilitado) o `"default"` (estándar). Solo usado por el proveedor MiMo |
| `models.<model>.supports_vision` | Soporte de imagen por modelo (retorna al predeterminado del proveedor) |
| `models.<model>.supports_video` | Soporte de video por modelo (retorna al predeterminado del proveedor) |
| `models.<model>.visible` | Si se muestra en `/v1/models` y el panel principal (predeterminado `true`) |
| `non_vision_image_policy` | Manejo de imágenes para modelos sin visión: `replace` (marcador) / `drop` / `reject` (error) |

### Estructura del proyecto

```
anthro-bridge/
├── README.md
├── SPEC.md                    Especificación
├── docs/
│   ├── README.ja.md           Japonés
│   ├── README.zh-CN.md        Chino (Simplificado)
│   ├── README.de.md           Alemán
│   ├── README.es.md           Español
│   ├── SPEC.ja.md             Japonés
│   ├── SPEC.zh-CN.md          Chino (Simplificado)
│   ├── SPEC.de.md             Alemán
│   ├── SPEC.es.md             Español
│   ├── THIRD_PARTY_INFERENCE.md   Guía de inferencia de terceros
│   ├── THIRD_PARTY_INFERENCE.ja.md
│   ├── THIRD_PARTY_INFERENCE.zh-CN.md
│   ├── THIRD_PARTY_INFERENCE.de.md
│   └── THIRD_PARTY_INFERENCE.es.md
├── LICENSE                    Licencia MIT
├── config.json                Configuración del proveedor
├── .gitignore
└── gui/
    ├── src/                   Frontend React (TypeScript)
    │   ├── components/        Componentes de UI
    │   ├── hooks/             Hooks personalizados
    │   └── i18n/              Soporte multilingüe
    │       └── lang/          Archivos de idioma (en, ja, zh-CN, zh-TW, ko, fr, de, es)
    ├── src-tauri/             Backend Tauri (Rust)
    │   ├── src/
    │   │   ├── lib.rs         24 comandos Tauri + ciclo de vida del proxy
    │   │   ├── main.rs        Punto de entrada
    │   │   └── proxy.rs       Servidor proxy axum
    │   ├── resources/
    │   │   └── config.json    Configuración incluida
    │   └── Cargo.toml
    └── package.json
```

### Compilación de desarrollo

```bash
cd gui
npm install
npm run tauri build    # Compilación de producción
npm run tauri dev      # Modo desarrollo (HMR)
```

Requiere cadena de herramientas [Rust](https://rustup.rs/) stable y Node.js 24+.

### Solución de problemas

#### Puerto 4000 en uso

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

#### Imagen/video rechazada

DeepSeek no soporta imágenes ni videos. Las imágenes se reemplazan automáticamente con texto de marcador (`non_vision_image_policy: "replace"`). Para usar imágenes nativamente, cambie a MiniMax, Kimi o MiMo (`claude-haiku-4-5`).

La ruta `claude-sonnet-4-6` de MiMo tampoco soporta imágenes — use `claude-haiku-4-5` para tareas de imagen. El video siempre es rechazado.

#### MiMo: Configuración de usuario existente no refleja los cambios

Si actualizó desde una versión anterior a v0.9.0, su configuración guardada puede tener los valores antiguos `"display_name": "MiMo"` o `"thinking": "default"`. v0.9.0 migra automáticamente estos en el primer inicio, pero si tiene problemas:

1. **Reinicie la aplicación** — la migración automática se ejecuta al iniciar.
2. **Restablecer configuración**: Elimine `%APPDATA%\Anthro Bridge\config.json` y reinicie. Se usará la configuración incluida con los ajustes correctos de MiMo.
3. **Verificación manual**: Abra `%APPDATA%\Anthro Bridge\config.json` y verifique que `providers.mimo` tenga `"display_name": "MiMo / Xiaomi"`, `"api_key_env": "XIAOMI_API_KEY"` y `thinking_mode` (no `thinking`) en las entradas de modelo.

### Prueba manual — MiMo / Xiaomi

#### Solo texto (claude-sonnet-4-6 → mimo-v2.5-pro)

1. Configure `XIAOMI_API_KEY` en Configuración → pestaña API Key → Guardar.
2. Seleccione **MiMo / Xiaomi** en el panel principal.
3. Inicie la pasarela.
4. Envíe un mensaje a través de Claude Desktop. Verifique que la respuesta llegue con bloques thinking.

#### Prueba de imagen (claude-haiku-4-5 → mimo-v2.5)

1. Seleccione **MiMo / Xiaomi** en el panel principal.
2. En Claude Desktop, adjunte una imagen a un mensaje y envíelo.
3. Verifique que la imagen se reciba y se describa correctamente.
4. Enviar una imagen a `claude-sonnet-4-6` debería resultar en un reemplazo de texto de marcador.

#### Verificación

Revise el panel de registro en la GUI — las solicitudes deberían mostrar el campo `model` reescrito a `mimo-v2.5-pro` o `mimo-v2.5` según la ruta.

### Licencia

MIT — consulte [LICENSE](LICENSE) para detalles.
