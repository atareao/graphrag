# Fix: NER descarta entidades cuando el LLM devuelve arrays anidados

## Why

`parse_entities_json` (`src/ner/ollama_ner.rs`) deserializa la respuesta del modelo como
`Vec<serde_json::Value>`. Algunos modelos —el comentario cita `llama3.2:3b`— envuelven el array
de entidades en otro array: `[[{..},{..}]]`. La deserialización tiene éxito, pero produce un `Vec`
cuyo único elemento es **otro array**. Después, `raw_entities_to_entities` hace `raw.get("label")`
sobre ese array, obtiene `None`, y **descarta todas las entidades**. El resultado es un `build` que
termina "correctamente" pero sin entidades extraídas, de forma silenciosa y dependiente del modelo.

## What Changes

- Añadir el helper `flatten_values` que aplana **recursivamente** arrays JSON anidados
  (`[[{..}]] → [{..}]`), conservando los objetos tal cual.
- Aplicarlo en las tres estrategias de `parse_entities_json`:
  1. array JSON directo,
  2. objeto con campo `entities`/`results`,
  3. array extraído por regex (modelos "thinking" / markdown).
- Añadir tests unitarios del parseo: array plano (sin cambio), anidado de un nivel, anidamiento
  profundo, objeto wrapper con array anidado, y respuesta vacía.

## Capabilities

### New Capabilities

- `entity-extraction`: parseo robusto de la respuesta JSON del LLM para extracción de entidades
  (NER), tolerante a arrays anidados en cualquiera de las estrategias de parseo.

### Modified Capabilities

<!-- Ninguna: `graph-build` describe el pipeline y el prune, pero no el parseo JSON del NER;
     el comportamiento de build/search no cambia, solo se recuperan entidades que antes se perdían. -->

## Impact

| Área | Impacto |
|------|---------|
| `src/ner/ollama_ner.rs` | `parse_entities_json` usa el nuevo helper `flatten_values` en las 3 estrategias |
| Tests | nuevos tests unitarios en el módulo `tests` de `ollama_ner.rs` |
| Compatibilidad | las respuestas planas siguen comportándose exactamente igual; solo se añade tolerancia a anidamiento |
| Dependencias / esquema / CLI | sin cambios |

## Out of scope

- No se cambia el prompt de NER, ni el modelo por defecto (`llama3.2:3b`), ni el batch mode.
- No se cambia el filtrado por `score >= 0.5` ni la deduplicación por `label`.
- No se añade detección de entidades alterna (GLiNER u otras).
