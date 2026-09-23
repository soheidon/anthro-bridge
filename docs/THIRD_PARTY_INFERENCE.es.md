[English](THIRD_PARTY_INFERENCE.md) | [日本語](THIRD_PARTY_INFERENCE.ja.md) | [中文(简体)](THIRD_PARTY_INFERENCE.zh-CN.md) | [中文(繁體)](THIRD_PARTY_INFERENCE.zh-TW.md) | [한국어](THIRD_PARTY_INFERENCE.ko.md) | [Français](THIRD_PARTY_INFERENCE.fr.md) | [Deutsch](THIRD_PARTY_INFERENCE.de.md) | [Español](THIRD_PARTY_INFERENCE.es.md)

# Usar Anthro Bridge con Claude Desktop / Cowork on 3P

Anthro Bridge se puede usar como una puerta de enlace local compatible con Anthropic para
Claude Desktop / Cowork on 3P.

Claude Desktop / Cowork on 3P admite inferencia de terceros a través de
la ventana de configuración integrada.

Documentación oficial:

- [https://claude.com/docs/cowork/3p/installation](https://claude.com/docs/cowork/3p/installation)
- [https://claude.com/docs/cowork/3p/configuration](https://claude.com/docs/cowork/3p/configuration)

## 1. Iniciar Anthro Bridge

Inicie Anthro Bridge primero y asegúrese de que la puerta de enlace esté en ejecución.

Por defecto, Anthro Bridge escucha en:

```text
http://127.0.0.1:4000
```

Mantenga Anthro Bridge abierto mientras usa Claude Desktop / Cowork on 3P.

## 2. Activar el modo de desarrollador en Claude Desktop

Abra Claude Desktop.

En Windows, abra el menú de la aplicación en la esquina superior izquierda.

Luego seleccione:

```text
Help → Troubleshooting → Enable Developer Mode
```

Después de activar el modo de desarrollador, aparecerá un nuevo menú `Developer`.

## 3. Abrir la configuración de inferencia de terceros

Abra:

```text
Developer → Configure third-party inference
```

Esto abre la ventana de configuración de inferencia de terceros.

## 4. Configurar la conexión

En la sección `Connection`, seleccione:

```text
Gateway
```

Luego ingrese los siguientes valores.

| Campo                 | Valor                                      |
| --------------------- | ------------------------------------------ |
| Gateway base URL      | `http://127.0.0.1:4000`                    |
| Gateway API key       | `sk-local-gateway`                         |
| Gateway auth scheme   | `bearer`                                   |
| Gateway extra headers | Déjelo en blanco a menos que necesite encabezados personalizados |

La `Gateway API key` debe coincidir con la API key local configurada en Anthro Bridge.

## 5. Configurar identidad y modelos

En la sección `Identity & Models`, agregue los IDs de modelo que Claude Desktop debe mostrar en el selector de modelos.

Ejemplo:

```text
claude-sonnet-4-6
claude-haiku-4-5
```

También puede asignar una etiqueta de visualización a cada modelo.

Ejemplo:

| ID del modelo        | Etiqueta          |
| -------------------- | ----------------- |
| `claude-sonnet-4-6`  | `Gateway Pro`     |
| `claude-haiku-4-5`   | `Gateway Flash`   |

El primer modelo de la lista se usa como entrada predeterminada del selector.

Para cada modelo, expanda la fila y confirme que el `Model ID` sea exactamente el nombre del modelo que desea que Claude Desktop envíe a Anthro Bridge.

Solo active `Offer 1M-context variant` si su proveedor upstream y el modelo seleccionado realmente admiten la ventana de contexto extendida.

## 6. Ejemplo de configuración

La configuración anterior corresponde a la siguiente configuración de inferencia de terceros:

```json
{
  "inferenceProvider": "gateway",
  "inferenceGatewayBaseUrl": "http://127.0.0.1:4000",
  "inferenceGatewayApiKey": "sk-local-gateway",
  "inferenceGatewayAuthScheme": "bearer",
  "inferenceModels": [
    {
      "name": "claude-sonnet-4-6",
      "labelOverride": "Gateway Pro"
    },
    {
      "name": "claude-haiku-4-5",
      "labelOverride": "Gateway Flash"
    }
  ]
}
```

## 7. Aplicar y reiniciar Claude Desktop

Después de configurar la puerta de enlace y la lista de modelos, aplique la configuración localmente.

Reinicie Claude Desktop si se le solicita.

Una vez que Claude Desktop se reinicie, las solicitudes de Cowork on 3P se enviarán a Anthro Bridge. Anthro Bridge luego enruta las solicitudes al proveedor upstream configurado en Anthro Bridge.

## Proveedor Xiaomi MiMo

Cuando Anthro Bridge está configurado para usar el proveedor Direct MiMo, las solicitudes de inferencia de Claude Desktop se enrutan a la API MiMo de Xiaomi.

### Modelos MiMo-V2.6

| Model ID | Nombre de visualización | Multimodal |
| --- | --- | --- |
| `mimo-v2.6-flash` | MiMo-V2.6-Flash | Image + Video |
| `mimo-v2.6-pro` | MiMo-V2.6-Pro | Image + Video |
| `mimo-v2.6-pro-ultraspeed` | MiMo-V2.6-Pro-UltraSpeed | Image + Video |

Todos los modelos V2.6 admiten una ventana de contexto de 1 000 000 tokens.

### Normal / Thinking

MiMo usa un simple interruptor Normal/Thinking. No hay niveles de esfuerzo de razonamiento (Low/Medium/High).

| Configuración | Parámetro upstream | Comportamiento |
| --- | --- | --- |
| Normal | `thinking: {"type": "disabled"}` | Inferencia estándar |
| Thinking | `thinking: {"type": "enabled"}` | Pensamiento extendido |

### Rutas predeterminadas

Las rutas integradas del preset Direct MiMo:

| Modelo de pasarela | Modelo upstream | Thinking |
| --- | --- | --- |
| `claude-opus-5` | `mimo-v2.6-pro` | Thinking |
| `claude-sonnet-5` | `mimo-v2.6-pro` | Normal |
| `claude-haiku-4-5` | `mimo-v2.6-flash` | Thinking |

### Compatibilidad con V2.5

Las configuraciones guardadas de `mimo-v2.5`, `mimo-v2.5-pro` y `mimo-v2.5-pro-ultraspeed` se conservan y siguen funcionando. Al iniciar, solo las rutas predeterminadas de fábrica sin modificar se migran automáticamente a V2.6. Las rutas personalizadas, la configuración del modo Thinking y los valores personalizados de `default_model` nunca se sobrescriben.

## Notas

Anthro Bridge es una puerta de enlace local no oficial compatible con Anthropic.

No está afiliado con Anthropic, Moon Bridge o ningún proveedor de modelos upstream.

Claude Desktop / Cowork on 3P se configura a través de la configuración de inferencia de terceros de Claude. Las etiquetas de menú y los campos de configuración pueden cambiar a medida que Anthropic actualiza Claude Desktop.
