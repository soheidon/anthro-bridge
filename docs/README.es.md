[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

Anthro Bridge es una puerta de enlace local y herramienta de configuracion de escritorio que permite a Claude Desktop y Claude Code usar multiples proveedores de LLM de terceros a traves de una API compatible con Anthropic.

La aplicacion consiste en:

- Un servidor proxy local escrito en Rust
- Una GUI nativa de Windows construida con Tauri 2, React y TypeScript
- Enrutamiento basado en modelos desde nombres de modelo Anthropic hacia modelos upstream especificos del proveedor
- Configuracion de modelo, razonamiento y capacidades por ruta

Anthro Bridge es un proyecto independiente. No es un fork, frontend ni aplicacion complementaria de Moon Bridge.

## Modelos soportados

Anthro Bridge soporta dos categorias de modelos upstream.

### Integraciones nativas

Estos proveedores son soportados a traves de sus propias APIs compatibles con Anthropic. No se requiere una cuenta de OpenRouter.

| Proveedor | Familias de modelos soportadas | Conexion |
|---|---|---|
| DeepSeek | DeepSeek V4 Pro y V4 Flash | API directa del proveedor |
| MiniMax | Variantes MiniMax M3 y M2.7 | API directa del proveedor |
| Kimi / Moonshot | Kimi K2.x y Kimi K3 | API directa del proveedor |
| MiMo / Xiaomi | Variantes MiMo V2.5 y V2.5 Pro | API directa del proveedor |

### Modelos soportados a traves de OpenRouter

Estos modelos se acceden a traves de un perfil de OpenRouter. Cada perfil tiene su propia clave API, mapeos de ruta y configuracion de razonamiento.

| Proveedor o familia de modelos | Soporte integrado | Controles de razonamiento |
|---|---|---|
| Poolside Laguna S 2.1 / Laguna XS 2.1 | Si | Controles de Thinking especificos del modelo |
| Tencent Hy3 | Si | Esfuerzo de razonamiento bajo y alto |
| InclusionAI Ring | Si | Controles de Thinking y razonamiento especificos del modelo |
| StepFun Step 3.5 / Step 3.7 | Si | Bajo, Medio y Alto donde sea soportado |
| Familia InclusionAI Ling | Si | Controles de Thinking especificos del modelo |
| OpenAI GPT-5.6 Sol / Terra / Luna | Si | Controles de Thinking y razonamiento especificos del modelo |

Otros modelos de OpenRouter tambien pueden seleccionarse desde la lista de modelos en vivo de OpenRouter o ingresarse manualmente. El soporte integrado significa que Anthro Bridge ya conoce la familia del modelo, las banderas de capacidad, la agrupacion de proveedores y el comportamiento de los controles de razonamiento.

## Como funciona

Claude Desktop y Claude Code envian solicitudes usando nombres de modelo Anthropic como:

- `claude-opus-5`
- `claude-sonnet-5`
- `claude-haiku-4-5`

Anthro Bridge trata estos nombres como identificadores de ruta estables. La GUI determina que proveedor y modelo upstream usa cada ruta.

Ejemplo:

```text
Solicitud de Claude Code
  model: claude-sonnet-5

Ruta de Anthro Bridge
  provider: perfil de OpenRouter "Hy3"
  upstream model: tencent/hunyuan-a13b-instruct
  reasoning mode: high
```

Solo se modifican los campos que deben adaptarse para el proveedor upstream. Los mensajes, llamadas a herramientas, resultados de herramientas, bloques de thinking y datos de streaming se preservan siempre que la API upstream los soporte.

## Caracteristicas principales

### Enrutamiento de proveedores

Anthro Bridge soporta dos tipos de conexion upstream:

1. **Integraciones directas de proveedores**, que se conectan a la propia API compatible con Anthropic de un proveedor.
2. **Perfiles de OpenRouter**, que se conectan a OpenRouter y pueden enrutar a multiples proveedores y familias de modelos a traves de una sola API.

#### Integraciones directas de proveedores

| ID del proveedor | Nombre mostrado | Endpoint predeterminado |
|---|---|---|
| `deepseek` | DeepSeek | `https://api.deepseek.com/anthropic` |
| `minimax` | MiniMax | `https://api.minimax.io/anthropic` |
| `kimi` | Kimi / Moonshot | `https://api.moonshot.cn/anthropic` |
| `mimo` | MiMo / Xiaomi | `https://api.xiaomimimo.com/anthropic` |

#### Integracion con OpenRouter

| Tipo de conexion | Nombre mostrado | Endpoint |
|---|---|---|
| Puerta de enlace de modelos multi-perfil | OpenRouter | `https://openrouter.ai/api/v1` |

OpenRouter no se trata como un unico proveedor de modelos. Cada perfil de OpenRouter puede seleccionar modelos de forma independiente de grupos de proveedores soportados como Poolside, Tencent, InclusionAI y StepFun, asi como otros modelos descubiertos desde la API de OpenRouter o ingresados manualmente.

Cada ruta Anthropic puede mapearse independientemente a un modelo de proveedor directo o a un modelo seleccionado a traves de un perfil de OpenRouter.

### Soporte multi-perfil de OpenRouter

Se pueden crear y gestionar multiples perfiles de OpenRouter de forma independiente.

Cada perfil tiene su propio:

- Nombre de perfil
- Configuracion de clave API
- Mapeos de ruta Opus, Sonnet y Haiku
- Configuracion de Thinking o razonamiento
- Lista de modelos de OpenRouter en cache

Los perfiles pueden agregarse, renombrarse, eliminarse y seleccionarse desde la GUI.

Los grupos de proveedores integrados de OpenRouter incluyen actualmente Poolside, Tencent, InclusionAI, StepFun y otras familias de modelos reconocidas. Los modelos desconocidos permanecen disponibles a traves de busqueda o entrada de modelo personalizada.

### Controles de modelo y razonamiento

Los controles disponibles dependen del modelo seleccionado.

Los controles soportados pueden incluir:

- Thinking activado o desactivado
- Modos de razonamiento normal, bajo, medio, alto, xhigh o maximo
- Esfuerzo de razonamiento especifico del proveedor
- Modos de razonamiento fijos para modelos que no permiten seleccion del usuario

Al cambiar de modelo, Anthro Bridge intenta preservar la configuracion de razonamiento compatible mas cercana. Si la configuracion anterior exacta no esta disponible, selecciona la opcion soportada mas cercana, prefiriendo la opcion mas debil cuando dos opciones son igualmente cercanas.

### Deteccion de capacidades

Anthro Bridge combina un registro de capacidades integrado con metadatos en vivo de OpenRouter.

Las capacidades pueden incluir:

- Entrada de imagenes
- Entrada de video
- Soporte de Thinking
- Soporte de esfuerzo de razonamiento
- Precios conocidos
- Reglas de traduccion de solicitudes especificas del proveedor

Los metadatos en vivo de OpenRouter se almacenan en cache para reducir las llamadas API innecesarias.

### Normalizacion del modelo en respuesta

Las APIs upstream a menudo devuelven su propio nombre de modelo en las respuestas. Anthro Bridge puede reescribir ese campo de vuelta al nombre de ruta Anthropic esperado por el cliente.

Por ejemplo:

```text
Modelo en respuesta upstream: deepseek-v4-pro
Modelo visible para el cliente: claude-sonnet-5
```

La normalizacion se aplica tanto a respuestas por streaming como sin streaming y puede activarse o desactivarse en Configuracion.

### Escrituras de configuracion serializadas

Las mutaciones de configuracion se serializan para evitar que escrituras concurrentes corrompan o reviertan la configuracion.

Esto cubre operaciones como:

- Cambios de modelo
- Cambios de modo de Thinking
- Cambios de esfuerzo de razonamiento
- Cambios en el perfil de OpenRouter
- Cambios de configuracion relacionados con claves API

### Cola de guardado de OpenRouter

Los cambios de ruta de OpenRouter se procesan a traves de una cola de guardado dedicada.

La cola proporciona:

- Operaciones de guardado serializadas
- Sustitucion de solicitudes obsoletas
- Identidad de ruta capturada cuando se envia una solicitud
- Proteccion contra closures de React obsoletos
- Proteccion contra reversion desde una ruta previamente seleccionada
- Reintento de actualizacion despues de un guardado exitoso
- Manejo agregado de reinicio de la puerta de enlace
- Procesamiento seguro de solicitudes agregadas durante el trabajo posterior al guardado

Esto evita que cambios rapidos de modelo, cambio de ruta o respuestas Tauri retrasadas restauren valores antiguos de la UI.

### Gestion de la puerta de enlace

La GUI proporciona:

- Controles de inicio y detencion de la puerta de enlace
- Seleccion de proveedor y perfil
- Configuracion de ruta
- Gestion de claves API
- Visualizacion de registros
- Actualizacion de lista de modelos
- Estado de guardado y visualizacion de errores

La puerta de enlace escucha en:

```text
http://127.0.0.1:4000
```

## Requisitos

- Windows 10 o Windows 11
- Node.js 24 o posterior para desarrollo
- Toolchain estable de Rust para desarrollo
- Una clave API para al menos un proveedor soportado

Una sola clave de proveedor es suficiente. No necesita claves para cada proveedor.

## Instalacion

Descargue el instalador de Windows mas reciente desde la pagina de Releases del proyecto y ejecutelo.

El instalador soporta:

- Ingles
- Japones
- Chino simplificado
- Chino tradicional
- Coreano
- Frances
- Aleman
- Espanol

Para actualizar Anthro Bridge, ejecute el instalador mas reciente. La configuracion existente del usuario se conserva.

La configuracion estable del usuario se almacena en:

```text
%APPDATA%\Anthro Bridge\
```

Las compilaciones de desarrollo usan una identidad de aplicacion y directorio de datos separados:

```text
%APPDATA%\Anthro Bridge Dev\
```

Esto permite que las versiones estable y de desarrollo coexistan sin compartir archivos de configuracion o cache.

## Inicio rapido

### 1. Configurar una clave API

Abra:

```text
Settings > API Key
```

Ingrese la clave del proveedor que planea usar y guardela.

Los nombres comunes de variables de entorno son:

| Proveedor | Variable de entorno |
|---|---|
| DeepSeek | `DEEPSEEK_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |
| MiMo / Xiaomi | `XIAOMI_API_KEY` |
| OpenRouter | `OPENROUTER_API_KEY` |

Los perfiles de OpenRouter pueden usar configuraciones de clave especificas del perfil gestionadas a traves de la GUI.

### 2. Configurar modelos de ruta

Abra Settings y seleccione el modelo upstream para cada ruta:

- Opus
- Sonnet
- Haiku

Para OpenRouter, seleccione o cree un perfil primero, luego configure cada ruta dentro de ese perfil.

### 3. Iniciar la puerta de enlace

Haga clic en **Start Gateway**.

Verifique que el endpoint local este disponible:

```text
GET http://127.0.0.1:4000/health
```

### 4. Configurar Claude Desktop o Claude Code

Dirija el cliente al endpoint de Anthro Bridge mientras continua usando nombres de modelo Anthropic.

Las instrucciones detalladas de inferencia de terceros estan disponibles en:

```text
docs/THIRD_PARTY_INFERENCE.md
```

## Endpoints de la API

| Metodo | Ruta | Descripcion |
|---|---|---|
| `GET` | `/health` | Verificacion de salud de la puerta de enlace |
| `GET` | `/v1/models` | Lista publica de modelos de ruta |
| `POST` | `/v1/messages` | API de mensajes con y sin streaming |
| `POST` | `/v1/messages/count_tokens` | Conteo de tokens cuando es soportado por el proveedor seleccionado |

## Configuracion

El archivo de configuracion principal es `config.json`.

La mayoria de las configuraciones deben modificarse a traves de la GUI. La edicion manual esta destinada para uso avanzado.

Los campos importantes del modelo incluyen:

| Clave | Descripcion |
|---|---|
| `models.<route>.upstream_model` | Nombre del modelo upstream enviado al proveedor |
| `models.<route>.thinking_mode` | Modo de Thinking especifico de la ruta |
| `models.<route>.reasoning_effort` | Esfuerzo de razonamiento especifico del proveedor |
| `models.<route>.supports_vision` | Anulacion de soporte de imagenes |
| `models.<route>.supports_video` | Anulacion de soporte de video |
| `models.<route>.visible` | Si la ruta se expone a los clientes y al panel principal |
| `non_vision_image_policy` | Como se maneja la entrada de imagen no soportada |
| `normalize_response_model_identity` | Si los nombres de modelo en respuesta se normalizan |

Las imagenes no soportadas pueden manejarse mediante una de las siguientes politicas:

- `replace`: reemplazar la imagen con un marcador de texto
- `drop`: eliminar el contenido de la imagen
- `reject`: devolver un error

## Notas de proveedores

### DeepSeek

`reasoning_effort` (esfuerzo de razonamiento):

- `deepseek-v4-pro`
  - Normal: esfuerzo de razonamiento desactivado
  - Thinking: High / Max
- `deepseek-v4-flash`
  - Normal: esfuerzo de razonamiento desactivado
  - Thinking: Low / High / Max

Al iniciar, un esfuerzo `low` o `medium` heredado almacenado para una ruta DeepSeek V4 Pro se migra a `high` (en linea con los niveles efectivos oficiales).

Enrutamiento DeepSeek predeterminado para instalaciones nuevas y configuraciones recien generadas:

- Opus 5 → V4 Flash, Thinking, Max
- Sonnet 5 → V4 Flash, Thinking, High
- Haiku 4.5 → V4 Flash, Thinking, Low

El enrutamiento guardado existente no se cambia automaticamente.

### MiniMax

El comportamiento del modelo MiniMax difiere segun la generacion del modelo. Anthro Bridge aplica el formato de solicitud requerido por el modelo seleccionado, incluyendo Thinking adaptativo o desactivado cuando es soportado.

### Kimi

Los modelos Kimi pueden usar un parametro de thinking o un modo de esfuerzo de razonamiento fijo dependiendo de la familia del modelo. Anthro Bridge traduce la seleccion de la GUI al formato de solicitud upstream apropiado.

### MiMo

MiMo usa `thinking_mode` en lugar del campo generico `thinking` para las rutas soportadas.

El soporte de vision varia segun el modelo. Anthro Bridge aplica la politica de imagen no soportada configurada cuando una ruta no puede aceptar entrada de imagen.

### OpenRouter

Los modelos de OpenRouter se agrupan por proveedor cuando son reconocidos. La GUI proporciona:

- Busqueda de modelos
- Agrupacion por proveedor
- Entrada de modelo personalizada
- Insignias de capacidad
- Visualizacion de precios
- Controles de razonamiento por modelo
- Actualizacion unificada de lista de modelos

Las capacidades y el comportamiento de los modelos de OpenRouter pueden cambiar con el tiempo. Se utilizan metadatos en vivo cuando estan disponibles, mientras que el registro integrado proporciona valores predeterminados estables para modelos conocidos.

## Interfaz de usuario

La interfaz de Settings incluye:

- Secciones de proveedores colapsables
- Configuracion de rutas Opus, Sonnet y Haiku
- Busqueda de modelos y agrupacion por proveedor para OpenRouter
- Controles de Thinking y razonamiento basados en la capacidad del modelo
- Entrada de modelo upstream personalizado
- Guardado automatico de ruta
- Guardado explicito de clave API
- Mensajes de progreso y error de guardado
- Informacion de precios y capacidad del modelo
- Interruptor de normalizacion de modelo en respuesta

El panel principal incluye:

- Seleccion de proveedor o perfil de OpenRouter
- Estado de la puerta de enlace
- Mapeos de ruta actuales
- Indicadores de capacidad
- Informacion de precios
- Estado de cambio de proveedor

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
│   │   │   └── paths.rs
│   │   └── resources/
│   └── package.json
└── LICENSE
```

### Ejecutar en modo desarrollo

```bash
cd gui
npm install
npm run tauri dev
```

### Compilar la variante de desarrollo

En Windows, use un solo trabajo de compilacion de Rust para evitar la terminacion intermitente del compilador:

```powershell
cd gui
$env:CARGO_BUILD_JOBS = "1"
npm run tauri:build:dev
Remove-Item Env:CARGO_BUILD_JOBS
```

Las compilaciones de desarrollo usan:

- Titulo de ventana: `Anthro Bridge (DEV)`
- Puerto: `4000`
- Identidad de aplicacion: `com.soheidon.anthro-bridge.dev`
- Directorios de configuracion y cache separados

### Compilaciones estables

Las compilaciones estables deben crearse solo para la preparacion de lanzamientos. El trabajo normal de implementacion y verificacion debe usar la variante de desarrollo.

## Verificacion

Verificacion del frontend:

```bash
cd gui
npx vitest run
npx tsc --noEmit
```

Verificacion de Rust:

```bash
cd gui/src-tauri
cargo check
```

Para el selector de ruta de OpenRouter especificamente:

```bash
cd gui
npx vitest run src/components/OpenRouterModelSelector.test.tsx
```

Las pruebas del selector de OpenRouter cubren:

- Identidad de ruta capturada durante guardados en cola
- Proteccion contra reversion entre rutas
- Proteccion contra callbacks obsoletos
- Comportamiento de reintento de actualizacion
- Reinicio de la puerta de enlace despues de fallo de actualizacion
- Sustitucion de solicitudes en vuelo
- Supresion de reversion basada en generacion

Se puede agregar una prueba dedicada de guardado multiple para la agregacion de reinicio para asegurar el siguiente comportamiento:

```text
guardado 1 solicita reinicio
guardado 2 no solicita reinicio
resultado: reiniciar una vez despues del lote
```

## Lista de verificacion manual

Las pruebas automatizadas no reproducen todas las condiciones de temporizacion de Tauri y React. Antes del lanzamiento, verifique lo siguiente en la compilacion de desarrollo:

- Cada perfil de OpenRouter muestra los detalles correctos al pasar el cursor
- La seleccion de modelo no revierte visiblemente despues de un cambio
- Las selecciones de Thinking y razonamiento permanecen estables despues de guardar
- La configuracion permanece correcta despues de cerrar y reabrir la pantalla de configuracion
- La configuracion permanece correcta despues de reiniciar la aplicacion
- Cambiar de perfil durante un guardado no corrompe ningun perfil
- Un guardado fallido revierte solo la ruta que lo inicio
- El reintento de actualizacion exitoso limpia el error anterior
- El reintento de actualizacion fallido deja visible el ultimo error
- El reinicio requerido de la puerta de enlace ocurre una vez despues del lote
- Los modelos personalizados se guardan y recargan correctamente
- Las capacidades integradas y en vivo de OpenRouter se muestran correctamente

## Solucion de problemas

### El puerto 4000 ya esta en uso

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### Un modelo rechaza entrada de imagen o video

Las capacidades del modelo varian segun el proveedor y la ruta. Verifique las insignias de capacidad en la GUI y seleccione una ruta compatible.

Para entrada de imagen no soportada, Anthro Bridge sigue `non_vision_image_policy`.

### La configuracion se revierte despues de una actualizacion

Reinicie la aplicacion primero para que las migraciones puedan ejecutarse.

Si el problema persiste:

1. Haga una copia de seguridad de la configuracion del usuario.
2. Compárela con la configuracion incluida.
3. Elimine campos obsoletos o restablezca la configuracion del usuario si es necesario.

Ubicacion de la configuracion estable:

```text
%APPDATA%\Anthro Bridge\config.json
```

Ubicacion de la configuracion de desarrollo:

```text
%APPDATA%\Anthro Bridge Dev\config.json
```

### La lista de modelos de OpenRouter esta desactualizada

Use el control de actualizacion unificada de modelos en Settings. Anthro Bridge almacena en cache los metadatos de modelos, por lo que puede ser necesaria una actualizacion manual despues de que OpenRouter cambie una entrada de modelo.

## Traduccion

El ingles es el README de origen.

Los archivos README traducidos se almacenan en `docs/`. Cuando el README en ingles cambie, regenere o actualice los archivos traducidos desde la fuente en ingles en lugar de editar cada idioma de forma independiente.

Los archivos de idioma para la UI de la aplicacion se almacenan en:

```text
gui/src/i18n/lang/
```

## Licencia

Licencia MIT. Consulte [LICENSE](LICENSE).
