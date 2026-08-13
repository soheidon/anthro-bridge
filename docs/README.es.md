[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

**Versión actual: 0.16.0**

Anthro Bridge es una pasarela local y una herramienta de configuración de escritorio que permite a Claude Desktop y Claude Code usar múltiples proveedores de LLM de terceros a través de una API compatible con Anthropic.

La aplicación consiste en:

- Un servidor proxy local escrito en Rust
- Una GUI nativa de Windows construida con Tauri 2, React y TypeScript
- Enrutamiento basado en modelos, desde los nombres de modelo de Anthropic hacia los modelos upstream específicos de cada proveedor
- Configuración de modelo, razonamiento y capacidades por ruta

Anthro Bridge es un proyecto independiente. No es un fork, un frontend ni una aplicación complementaria de Moon Bridge.

## Novedades de la versión 0.16.0

La versión 0.16.0 añade gestión del contexto de Claude Code con conocimiento de los modelos.

- Anthro Bridge resuelve la capacidad de contexto de los modelos upstream asignados a las rutas Opus, Sonnet y Haiku.
- En modo automático, la capacidad conocida más pequeña entre las tres rutas se usa como ventana de contexto segura de Claude Code.
- El control del contexto solo se aplica cuando se conocen las tres capacidades de ruta.
- El encabezado ofrece un interruptor compacto de gestión del contexto; el modo avanzado y los valores de umbral siguen disponibles a través de `config.json`.
- La aplicación puede generar un comando de lanzamiento completo de PowerShell que contiene las variables de conexión de Anthro Bridge y las variables de control del contexto de Claude Code.
- Cuando la gestión del contexto está desactivada o incompleta, el comando generado elimina las variables de control del contexto obsoletas de la sesión de PowerShell actual.
- Los metadatos de contexto integrados cubren los modelos estándar de los proveedores directos y los modelos integrados de OpenRouter.
- El comando generado y su comportamiento con las variables de entorno están cubiertos por pruebas unitarias de Rust, pruebas de integración de PowerShell de Windows y pruebas de flujo de copia del frontend.

## Modelos soportados

Anthro Bridge soporta dos categorías de modelos upstream.

### Integraciones nativas

Estos proveedores son compatibles a través de sus propias APIs compatibles con Anthropic. No se requiere una cuenta de OpenRouter.

| Proveedor | Familias de modelos soportadas | Conexión |
|---|---|---|
| DeepSeek | DeepSeek V4 Pro y V4 Flash | API directa del proveedor |
| MiniMax | Variantes MiniMax M3 y M2.7 | API directa del proveedor |
| Kimi / Moonshot | Kimi K2.x y Kimi K3 | API directa del proveedor |
| MiMo / Xiaomi | Variantes MiMo V2.5 y V2.5 Pro | API directa del proveedor |

### Modelos soportados a través de OpenRouter

Estos modelos se acceden a través de un perfil de OpenRouter. Cada perfil tiene su propia clave API, sus mapeos de ruta y su configuración de razonamiento.

| Proveedor o familia de modelos | Soporte integrado | Controles de razonamiento |
|---|---|---|
| Poolside Laguna S 2.1 / Laguna XS 2.1 | Sí | Controles de Thinking específicos del modelo |
| Tencent Hy3 | Sí | Nivel de razonamiento bajo y alto |
| InclusionAI Ring | Sí | Controles de Thinking y razonamiento específicos del modelo |
| StepFun Step 3.5 / Step 3.7 | Sí | Bajo, Medio y Alto donde sea compatible |
| Familia InclusionAI Ling | Sí | Controles de Thinking específicos del modelo |
| OpenAI GPT-5.6 Sol / Terra / Luna | Sí | Controles de Thinking y razonamiento específicos del modelo |

Otros modelos de OpenRouter también pueden seleccionarse desde la lista de modelos en vivo de OpenRouter o ingresarse manualmente. El soporte integrado significa que Anthro Bridge ya conoce la familia del modelo, las banderas de capacidad, la agrupación por proveedor y el comportamiento de los controles de razonamiento.

## Cómo funciona

Claude Desktop y Claude Code envían solicitudes usando nombres de modelo Anthropic como:

- `claude-opus-5`
- `claude-sonnet-5`
- `claude-haiku-4-5`

Anthro Bridge trata estos nombres como identificadores de ruta estables. La GUI determina qué proveedor y modelo upstream usa cada ruta.

Ejemplo:

```text
Claude Code request
  model: claude-sonnet-5

Anthro Bridge route
  provider: OpenRouter profile "Hy3"
  upstream model: tencent/hunyuan-a13b-instruct
  reasoning mode: high
```

Solo se modifican los campos que deben adaptarse al proveedor upstream. Los mensajes, las llamadas a herramientas, los resultados de herramientas, los bloques de thinking y los datos de streaming se preservan siempre que la API upstream los soporte.

## Características principales

### Enrutamiento de proveedores

Anthro Bridge soporta dos tipos de conexión upstream:

1. **Integraciones directas de proveedores**, que se conectan a la propia API compatible con Anthropic de un proveedor.
2. **Perfiles de OpenRouter**, que se conectan a OpenRouter y pueden enrutar a múltiples proveedores y familias de modelos a través de una sola API.

#### Integraciones directas de proveedores

| ID del proveedor | Nombre mostrado | Endpoint predeterminado |
|---|---|---|
| `deepseek` | DeepSeek | `https://api.deepseek.com/anthropic` |
| `minimax` | MiniMax | `https://api.minimax.io/anthropic` |
| `kimi` | Kimi / Moonshot | `https://api.moonshot.cn/anthropic` |
| `mimo` | MiMo / Xiaomi | `https://api.xiaomimimo.com/anthropic` |

#### Integración con OpenRouter

| Tipo de conexión | Nombre mostrado | Endpoint |
|---|---|---|
| Pasarela de modelos multiprofil | OpenRouter | `https://openrouter.ai/api/v1` |

OpenRouter no se trata como un único proveedor de modelos. Cada perfil de OpenRouter puede seleccionar modelos de forma independiente de grupos de proveedores compatibles como Poolside, Tencent, InclusionAI y StepFun, así como otros modelos descubiertos desde la API de OpenRouter o ingresados manualmente.

Cada ruta Anthropic puede mapearse de forma independiente a un modelo de proveedor directo o a un modelo seleccionado a través de un perfil de OpenRouter.

### Soporte de múltiples perfiles de OpenRouter

Se pueden crear y gestionar múltiples perfiles de OpenRouter de forma independiente.

Cada perfil tiene lo siguiente:

- Nombre de perfil
- Configuración de clave API
- Mapeos de ruta Opus, Sonnet y Haiku
- Configuración de Thinking o razonamiento
- Lista de modelos de OpenRouter en caché

Los perfiles pueden agregarse, renombrarse, eliminarse, reordenarse mediante arrastrar y soltar, ocultarse y seleccionarse desde la GUI. El panel principal muestra una tarjeta por cada perfil visible y conserva el orden guardado después de la actualización.

Los grupos de proveedores integrados de OpenRouter incluyen actualmente Poolside, Tencent, InclusionAI, StepFun, OpenAI GPT-5.6 y otras familias de modelos reconocidas. Los modelos desconocidos permanecen disponibles a través de la búsqueda o de la entrada de modelos personalizados. El panel principal abrevia los IDs cualificados por proveedor, como `poolside/laguna-s-2.1`, a `laguna-s-2.1` para facilitar la lectura, conservando el ID completo para el enrutamiento.

### Precios de OpenRouter y detalles de modelos

El panel de precios de modelos de Configuración muestra los precios integrados de los modelos OpenRouter compatibles, incluidos los precios de entrada (prompt), de salida y de entrada en caché. Los precios promocionales pueden mostrarse junto con los precios estándar revisados, incluidas las variantes GPT-5.6 Sol, Terra y Luna y sus variantes Pro. Las notas de precios pueden incluir los precios de contexto largo cuando corresponda.

### Dimensionado adaptable del panel principal

La altura inicial de la ventana se calcula a partir del número de tarjetas de proveedor y de OpenRouter visibles en el panel principal de tres columnas. Las filas adicionales de tarjetas aumentan la altura de la ventana respetando el tamaño mínimo nativo, el área de trabajo del monitor, el escalado DPI y las decoraciones de la barra de título. Cuando cambia la visibilidad o el número de perfiles, la altura se recalcula para la nueva cantidad de filas; el redimensionado manual se conserva mientras la cantidad de filas permanezca igual.

### Instalador de Windows localizado

El instalador NSIS de Windows ofrece selección de idioma para inglés, japonés, chino simplificado, chino tradicional, coreano, francés, alemán y español. El instalador usa el icono de la aplicación Anthro Bridge y conserva la configuración estable del usuario durante las actualizaciones.

### Últimas mejoras de fiabilidad de la interfaz

Las escrituras de configuración están serializadas, los guardados de OpenRouter usan una ruta de actualización en cola con protección contra solicitudes obsoletas, y las operaciones de reordenación de perfiles se recuperan correctamente tras fallos de actualización. Las pruebas de regresión cubren el orden de los perfiles, las condiciones de carrera al guardar, los precios de los modelos, el conteo de tarjetas del panel principal y el dimensionado de la ventana.

### Controles de modelo y razonamiento

Los controles disponibles dependen del modelo seleccionado.

Los controles compatibles pueden incluir:

- Thinking activado o desactivado
- Modos de razonamiento normal, bajo, medio, alto, xhigh o máximo
- Nivel de razonamiento específico del proveedor
- Modos de razonamiento fijos para modelos que no permiten la selección del usuario

Al cambiar de modelo, Anthro Bridge intenta preservar la configuración de razonamiento compatible más cercana. Si la configuración anterior exacta no está disponible, selecciona la opción compatible más cercana, prefiriendo la opción más débil cuando dos opciones son igualmente cercanas.

### Detección de capacidades

Anthro Bridge combina un registro de capacidades integrado con metadatos en vivo de OpenRouter.

Las capacidades pueden incluir:

- Entrada de imágenes
- Entrada de video
- Soporte de Thinking
- Soporte de nivel de razonamiento
- Precios conocidos
- Reglas de traducción de solicitudes específicas del proveedor

Los metadatos en vivo de OpenRouter se almacenan en caché para reducir las llamadas API innecesarias.

### Normalización del modelo en la respuesta

Las APIs upstream a menudo devuelven su propio nombre de modelo en las respuestas. Anthro Bridge puede reescribir ese campo de vuelta al nombre de ruta Anthropic esperado por el cliente.

Por ejemplo:

```text
Upstream response model: deepseek-v4-pro
Client-visible model:    claude-sonnet-5
```

La normalización se aplica tanto a respuestas por streaming como sin streaming y puede activarse o desactivarse en Configuración.

### Escrituras de configuración serializadas

Las mutaciones de configuración se serializan para evitar que las escrituras concurrentes corrompan o reviertan la configuración.

Esto cubre operaciones como:

- Cambios de modelo
- Cambios de modo de Thinking
- Cambios de nivel de razonamiento
- Cambios en el perfil de OpenRouter
- Cambios de configuración relacionados con claves API

### Cola de guardado de OpenRouter

Los cambios de ruta de OpenRouter se procesan a través de una cola de guardado dedicada.

La cola proporciona:

- Operaciones de guardado serializadas
- Sustitución de solicitudes obsoletas
- Identidad de ruta capturada cuando se envía una solicitud
- Protección contra closures de React obsoletos
- Protección contra la reversión desde una ruta seleccionada anteriormente
- Reintento de actualización después de un guardado exitoso
- Manejo agregado del reinicio de la pasarela
- Procesamiento seguro de solicitudes agregadas durante el trabajo posterior al guardado

Esto evita que los cambios rápidos de modelo, el cambio de ruta o las respuestas Tauri retrasadas restauren valores antiguos de la interfaz.

### Gestión del contexto de Claude Code

Anthro Bridge 0.16.0 puede generar comandos de lanzamiento de Claude Code con ajustes de contexto que tienen en cuenta el modelo.

El resolver realiza los siguientes pasos:

1. Resolver el modelo upstream asignado a cada ruta canónica:
   - `claude-opus-5`
   - `claude-sonnet-5`
   - `claude-haiku-4-5`
2. Consultar la capacidad de contexto conocida de cada modelo upstream.
3. Exigir que se conozcan las tres capacidades de ruta.
4. Usar la capacidad más pequeña como ventana de contexto segura.
5. Aplicar el porcentaje de activación configurado.

Por ejemplo, si las tres rutas se resuelven a capacidades de 1.000.000, 262.144 y 1.000.000 tokens, Anthro Bridge usa:

```text
window: 262144
trigger override: 90%
estimated trigger point: 235929 tokens
```

El comando de PowerShell generado usa las variables oficiales de Claude Code:

```text
CLAUDE_CODE_AUTO_COMPACT_WINDOW
CLAUDE_AUTOCOMPACT_PCT_OVERRIDE
```

También incluye las variables de conexión de la pasarela de Anthro Bridge:

```text
ANTHROPIC_BASE_URL
ANTHROPIC_AUTH_TOKEN
```

Ejemplo:

```powershell
$env:ANTHROPIC_BASE_URL='http://127.0.0.1:4000'; $env:ANTHROPIC_AUTH_TOKEN='sk-local-gateway'; $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW='262144'; $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE='90'; claude
```

Cuando la gestión del contexto está desactivada, configurada como el comportamiento predeterminado de Claude Code, o incompleta porque se desconoce la capacidad de una ruta, el comando generado elimina las variables de contexto obsoletas antes de iniciar Claude Code:

```powershell
Remove-Item Env:CLAUDE_CODE_AUTO_COMPACT_WINDOW -ErrorAction SilentlyContinue;
Remove-Item Env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE -ErrorAction SilentlyContinue;
```

El porcentaje de anulación solicita una compactación proactiva más temprana. Claude Code puede ignorar los valores que retrasarían la compactación más allá de su propio comportamiento predeterminado.

Anthro Bridge verifica la generación del comando y la inyección de variables de entorno en PowerShell. Esto por sí solo no demuestra que una versión concreta de Claude Code haya consumido las variables; la confirmación final requiere los diagnósticos de Claude Code o la observación del comportamiento de compactación.

### Gestión de la pasarela

La GUI proporciona:

- Controles de inicio y detención de la pasarela
- Selección de proveedor y perfil
- Configuración de rutas
- Gestión de claves API
- Visualización de registros
- Actualización de la lista de modelos
- Visualización del estado de guardado y de errores

La pasarela escucha en:

```text
http://127.0.0.1:4000
```

## Requisitos

- Windows 10 o Windows 11
- Node.js 24 o posterior para desarrollo
- Toolchain estable de Rust para desarrollo
- Una clave API para al menos un proveedor compatible

Una sola clave de proveedor es suficiente. No necesita claves para cada proveedor.

## Instalación

Descargue el instalador de Windows más reciente desde la página de Releases del proyecto y ejecútelo.

El instalador admite:

- Inglés
- Japonés
- Chino simplificado
- Chino tradicional
- Coreano
- Francés
- Alemán
- Español

Para actualizar Anthro Bridge, ejecute el instalador más reciente. La configuración existente del usuario se conserva.

La configuración estable del usuario se almacena en:

```text
%APPDATA%\Anthro Bridge\
```

Las compilaciones de desarrollo usan una identidad de aplicación y un directorio de datos separados:

```text
%APPDATA%\Anthro Bridge Dev\
```

Esto permite que las versiones estable y de desarrollo coexistan sin compartir archivos de configuración o caché.

## Inicio rápido

### 1. Configurar una clave API

Abra:

```text
Settings > API Key
```

Ingrese la clave del proveedor que planea usar y guárdela.

Los nombres comunes de variables de entorno son:

| Proveedor | Variable de entorno |
|---|---|
| DeepSeek | `DEEPSEEK_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |
| MiMo / Xiaomi | `XIAOMI_API_KEY` |
| OpenRouter | `OPENROUTER_API_KEY` |

Los perfiles de OpenRouter pueden usar configuraciones de clave específicas del perfil gestionadas a través de la GUI.

### 2. Configurar los modelos de ruta

Abra Configuración y seleccione el modelo upstream para cada ruta:

- Opus
- Sonnet
- Haiku

Para OpenRouter, seleccione o cree un perfil primero y luego configure cada ruta dentro de ese perfil.

### 3. Iniciar la pasarela

Haga clic en **Iniciar pasarela**.

Verifique que el endpoint local esté disponible:

```text
GET http://127.0.0.1:4000/health
```

### 4. Iniciar Claude Code a través de Anthro Bridge

Abra el panel de configuración de Claude y haga clic en **Copiar comando de lanzamiento de Claude Code**.

Pegue el comando generado en PowerShell. El comando incluye:

- `ANTHROPIC_BASE_URL`
- `ANTHROPIC_AUTH_TOKEN`
- `CLAUDE_CODE_AUTO_COMPACT_WINDOW` cuando se aplica la gestión del contexto
- `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` cuando se aplica la gestión del contexto
- comandos de limpieza de las variables de contexto obsoletas cuando no se aplica la gestión del contexto

El comando inicia Claude Code con Anthro Bridge como pasarela, conservando el comportamiento de contexto configurado con conocimiento del modelo.

Para Claude Desktop y las instrucciones adicionales de inferencia de terceros, consulte:

```text
docs/THIRD_PARTY_INFERENCE.md
```

## Endpoints de la API

| Método | Ruta | Descripción |
|---|---|---|
| `GET` | `/health` | Verificación de salud de la pasarela |
| `GET` | `/v1/models` | Lista pública de modelos de ruta |
| `POST` | `/v1/messages` | API de mensajes con y sin streaming |
| `POST` | `/v1/messages/count_tokens` | Conteo de tokens cuando el proveedor seleccionado lo admite |

## Configuración

El archivo de configuración principal es `config.json`.

La mayoría de las configuraciones deben modificarse a través de la GUI. La edición manual está pensada para uso avanzado.

Los campos importantes del modelo incluyen:

| Clave | Descripción |
|---|---|
| `models.<route>.upstream_model` | Nombre del modelo upstream enviado al proveedor |
| `models.<route>.thinking_mode` | Modo de Thinking específico de la ruta |
| `models.<route>.reasoning_effort` | Nivel de razonamiento específico del proveedor |
| `models.<route>.supports_vision` | Anulación del soporte de imágenes |
| `models.<route>.supports_video` | Anulación del soporte de video |
| `models.<route>.visible` | Si la ruta se expone a los clientes y al panel principal |
| `non_vision_image_policy` | Cómo se maneja la entrada de imagen no compatible |
| `normalize_response_model_identity` | Si los nombres de modelo de las respuestas se normalizan |
| `claude_code.auto_compact.enabled` | Interruptor global de gestión del contexto |
| `claude_code.auto_compact.trigger_percent` | Porcentaje de compactación proactiva solicitado |
| `claude_code.auto_compact.mode` | `auto`, `manual` o `claude_default` |
| `claude_code.auto_compact.window_tokens` | Ventana de contexto manual usada en el modo `manual` |

Las imágenes no compatibles pueden manejarse mediante una de las siguientes políticas:

- `replace`: reemplazar la imagen con un marcador de texto
- `drop`: eliminar el contenido de la imagen
- `reject`: devolver un error

### Configuración de la gestión del contexto

La GUI expone solo el interruptor global de gestión del contexto. Los valores avanzados pueden editarse directamente en `config.json`.

Modo automático:

```json
{
  "claude_code": {
    "auto_compact": {
      "enabled": true,
      "mode": "auto",
      "trigger_percent": 90
    }
  }
}
```

Modo manual:

```json
{
  "claude_code": {
    "auto_compact": {
      "enabled": true,
      "mode": "manual",
      "window_tokens": 240000,
      "trigger_percent": 90
    }
  }
}
```

Comportamiento predeterminado de Claude Code:

```json
{
  "claude_code": {
    "auto_compact": {
      "enabled": true,
      "mode": "claude_default"
    }
  }
}
```

En modo `auto`, Anthro Bridge aplica las variables de contexto solo cuando las tres rutas canónicas tienen metadatos de contexto conocidos. Los modelos OpenRouter personalizados desconocidos siguen siendo destinos de enrutamiento válidos, pero la gestión del contexto informa de un estado incompleto hasta que haya metadatos disponibles o se configure el modo manual.

Las capacidades estáticas de los modelos se almacenan en:

```text
gui/src-tauri/resources/model_context_windows.json
```

El registro incluye los modelos estándar de DeepSeek, MiniMax, Kimi, MiMo, Poolside, Tencent, InclusionAI, StepFun y OpenAI GPT-5.6 usados por los preajustes integrados.

## Notas de proveedores

### DeepSeek

`reasoning_effort`:

- `deepseek-v4-pro` (V4-Pro-0813)
  - Normal: nivel de razonamiento desactivado
  - Thinking: Low / High / Max
- `deepseek-v4-flash` (V4-Flash-0731)
  - Normal: nivel de razonamiento desactivado
  - Thinking: Low / High / Max

Al iniciar, un nivel `medium` o `xhigh` heredado almacenado para una ruta DeepSeek V4 Pro se migra a `high` (en correspondencia con los niveles de razonamiento efectivos de DeepSeek). El proxy también normaliza los valores de esfuerzo antes de enviar (`medium`/`xhigh` → `high`) mediante el formato `output_config.effort`.

Enrutamiento predeterminado de DeepSeek para instalaciones nuevas y configuraciones recién generadas:

- Opus 5 → V4 Flash, Thinking, Max
- Sonnet 5 → V4 Flash, Thinking, High
- Haiku 4.5 → V4 Flash, Thinking, Low

El enrutamiento guardado existente no se cambia automáticamente.

### MiniMax

El comportamiento de los modelos MiniMax difiere según la generación del modelo. Anthro Bridge aplica el formato de solicitud requerido por el modelo seleccionado, incluido el Thinking adaptativo o desactivado cuando es compatible.

### Kimi

Los modelos Kimi pueden usar un parámetro de thinking o un modo de nivel de razonamiento fijo dependiendo de la familia del modelo. Anthro Bridge traduce la selección de la GUI al formato de solicitud upstream apropiado.

### MiMo

MiMo usa `thinking_mode` en lugar del campo genérico `thinking` para las rutas compatibles.

El soporte de visión varía según el modelo. Anthro Bridge aplica la política de imágenes no compatibles configurada cuando una ruta no puede aceptar entrada de imagen.

### OpenRouter

Los modelos de OpenRouter se agrupan por proveedor cuando son reconocidos. La GUI proporciona:

- Búsqueda de modelos
- Agrupación por proveedor
- Entrada de modelos personalizados
- Insignias de capacidad
- Visualización de precios
- Controles de razonamiento por modelo
- Actualización unificada de la lista de modelos

Las capacidades y el comportamiento de los modelos de OpenRouter pueden cambiar con el tiempo. Se utilizan metadatos en vivo cuando están disponibles, mientras que el registro integrado proporciona valores predeterminados estables para los modelos conocidos.

El perfil integrado OpenAI GPT-5.6 Balanced usa por defecto Thinking High en todas las rutas para instalaciones nuevas y configuraciones recién generadas:

- Opus 5 → GPT-5.6 Sol, Thinking, High
- Sonnet 5 → GPT-5.6 Terra, Thinking, High
- Haiku 4.5 → GPT-5.6 Luna, Thinking, High

El enrutamiento guardado existente no se cambia automáticamente.

## Interfaz de usuario

La interfaz de Configuración incluye:

- Secciones de proveedores plegables
- Configuración de rutas Opus, Sonnet y Haiku
- Búsqueda de modelos y agrupación por proveedor para OpenRouter
- Controles de Thinking y razonamiento basados en la capacidad del modelo
- Entrada de modelo upstream personalizado
- Guardado automático de rutas
- Guardado explícito de la clave API
- Mensajes de progreso y error de guardado
- Información de precios y capacidades del modelo
- Interruptor de normalización del modelo en la respuesta
- Interruptor de gestión del contexto de Claude Code en el encabezado
- Acción de copia del comando de lanzamiento de Claude Code en el panel de configuración de Claude

El panel principal incluye:

- Selección de proveedor o perfil de OpenRouter
- Estado de la pasarela
- Mapeos de ruta actuales
- Indicadores de capacidad
- Información de precios
- Estado del cambio de proveedor

## Desarrollo

### Estructura del proyecto

```text
anthro-bridge/
├── README.md
├── SPEC.md
├── config.json
├── docs/
│   ├── README.*.md
│   ├── SPEC.*.md
│   └── THIRD_PARTY_INFERENCE*.md
├── gui/
│   ├── src/
│   │   ├── components/
│   │   ├── hooks/
│   │   └── i18n/
│   ├── src-tauri/
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── main.rs
│   │   │   ├── proxy.rs
│   │   │   ├── openrouter.rs
│   │   │   ├── config_template.rs
│   │   │   ├── model_capabilities.rs
│   │   │   ├── model_routing.rs
│   │   │   └── paths.rs
│   │   └── resources/
│   │       ├── config.json
│   │       └── model_context_windows.json
│   └── package.json
└── LICENSE
```

### Ejecutar en modo de desarrollo

```bash
cd gui
npm install
npm run tauri dev
```

### Compilar la variante de desarrollo

En Windows, use un solo trabajo de compilación de Rust para evitar la terminación intermitente del compilador:

```powershell
cd gui
$env:CARGO_BUILD_JOBS = "1"
npm run tauri:build:dev
Remove-Item Env:CARGO_BUILD_JOBS
```

Las compilaciones de desarrollo usan:

- Título de ventana: `Anthro Bridge (DEV)`
- Puerto: `4000`
- Identidad de aplicación: `com.soheidon.anthro-bridge.dev`
- Directorios de configuración y caché separados

### Compilaciones estables

Las compilaciones estables deben crearse solo para la preparación de lanzamientos. El trabajo normal de implementación y verificación debe usar la variante de desarrollo.

## Verificación

Verificación del frontend:

```bash
cd gui
npx vitest run
npx tsc --noEmit
```

Verificación de Rust:

```bash
cd gui/src-tauri
cargo check
cargo test
```

La verificación de la gestión del contexto cubre:

- Resolución compartida de ruta a upstream entre el proxy y el resolver de contexto
- Metadatos de contexto de modelos completos para los modelos integrados de proveedores directos y de OpenRouter
- Selección automática de la ventana mínima entre las tres rutas canónicas
- Modos aplicado, desactivado, incompleto, manual y predeterminado de Claude
- Nombres oficiales de variables de entorno de Claude Code
- Representación y escape del comando de PowerShell
- Variables de conexión de la pasarela
- Inyección de variables de entorno en un proceso secundario real de PowerShell de Windows
- Eliminación de las variables de contexto obsoletas cuando no se aplica la gestión del contexto
- Copia en el frontend del comando de lanzamiento generado

Para el selector de ruta de OpenRouter específicamente:

```bash
cd gui
npx vitest run src/components/OpenRouterModelSelector.test.tsx
```

Las pruebas del selector de OpenRouter cubren:

- Identidad de ruta capturada durante los guardados en cola
- Protección contra la reversión entre rutas
- Protección contra callbacks obsoletos
- Comportamiento de reintento de actualización
- Reinicio de la pasarela tras un fallo de actualización
- Sustitución de solicitudes en vuelo
- Supresión de la reversión basada en generación

Puede agregarse una prueba dedicada de guardado múltiple para la agregación de reinicios a fin de fijar el siguiente comportamiento:

```text
save 1 requests restart
save 2 does not request restart
result: restart once after the batch
```

## Lista de verificación manual

Las pruebas automatizadas no reproducen todas las condiciones de temporización de Tauri y React. Antes del lanzamiento, verifique lo siguiente en la compilación de desarrollo:

- Cada perfil de OpenRouter muestra los detalles correctos al pasar el cursor
- La selección de modelo no revierte visiblemente después de un cambio
- Las selecciones de Thinking y razonamiento permanecen estables después de guardar
- La configuración permanece correcta después de cerrar y reabrir la pantalla de configuración
- La configuración permanece correcta después de reiniciar la aplicación
- Cambiar de perfil durante un guardado no corrompe ninguno de los perfiles
- Un guardado fallido revierte solo la ruta que lo inició
- El éxito del reintento de actualización limpia el error anterior
- El fallo del reintento de actualización deja visible el último error
- El reinicio requerido de la pasarela ocurre una vez después del lote
- Los modelos personalizados se guardan y recargan correctamente
- Las capacidades integradas y en vivo de OpenRouter se muestran correctamente
- El interruptor de gestión del contexto del encabezado usa un conmutador visual y conserva su estado
- Cada proveedor integrado o preajuste de OpenRouter resuelve las tres capacidades de ruta
- El comando de Claude Code generado contiene las variables de conexión de la pasarela
- Con la gestión del contexto activada, el comando generado contiene `CLAUDE_CODE_AUTO_COMPACT_WINDOW` y `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`
- Con la gestión del contexto desactivada, el comando generado elimina ambas variables de contexto
- El comando copiado inicia Claude Code a través de la pasarela Anthro Bridge en ejecución

## Solución de problemas

### El puerto 4000 ya está en uso

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### Un modelo rechaza la entrada de imagen o video

Las capacidades de los modelos varían según el proveedor y la ruta. Verifique las insignias de capacidad en la GUI y seleccione una ruta compatible.

Para la entrada de imagen no compatible, Anthro Bridge sigue `non_vision_image_policy`.

### La configuración se revierte después de una actualización

Reinicie la aplicación primero para que las migraciones puedan ejecutarse.

Si el problema persiste:

1. Haga una copia de seguridad de la configuración del usuario.
2. Compárela con la configuración incluida.
3. Elimine los campos obsoletos o restablezca la configuración del usuario si es necesario.

Ubicación de la configuración estable:

```text
%APPDATA%\Anthro Bridge\config.json
```

Ubicación de la configuración de desarrollo:

```text
%APPDATA%\Anthro Bridge Dev\config.json
```

### La lista de modelos de OpenRouter está desactualizada

Use el control de actualización unificada de modelos en Configuración. Anthro Bridge almacena en caché los metadatos de los modelos, por lo que puede ser necesaria una actualización manual después de que OpenRouter cambie una entrada de modelo.

### La gestión del contexto está incompleta

La gestión automática del contexto requiere capacidades conocidas para las tres rutas canónicas.

Verifique los modelos upstream configurados para Opus, Sonnet y Haiku. Un modelo personalizado o recién lanzado puede que aún no exista en `model_context_windows.json`.

Opciones:

1. Seleccione un modelo integrado con metadatos conocidos.
2. Agregue metadatos de modelo verificados al registro estático.
3. Use el modo manual en `config.json`.
4. Use `claude_default` para dejar la compactación por completo a Claude Code.

### Claude Code no usa la configuración de contexto esperada

Confirme que Claude Code se inició desde el comando de PowerShell generado y no desde un comando de terminal independiente.

En la misma sesión de PowerShell, inspeccione:

```powershell
echo $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW
echo $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE
echo $env:ANTHROPIC_BASE_URL
echo $env:ANTHROPIC_AUTH_TOKEN
```

Estos valores confirman que el entorno de lanzamiento se preparó. No demuestran que Claude Code haya consumido las variables. Use los diagnósticos de Claude Code u observe el comportamiento de compactación para la confirmación final.

## Traducción

El inglés es el README de origen.

Los archivos README traducidos se almacenan en `docs/`. Cuando el README en inglés cambie, regenere o actualice los archivos traducidos a partir de la fuente en inglés en lugar de editar cada idioma de forma independiente.

Los archivos de idioma de la interfaz de la aplicación se almacenan en:

```text
gui/src/i18n/lang/
```

## Licencia

Licencia MIT. Consulte [LICENSE](../LICENSE).
