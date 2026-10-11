# entity-extraction Specification

## Purpose

Robust parsing of the LLM (Ollama) JSON response during NER entity extraction, so that a valid
response is never silently discarded because of the shape of the JSON envelope. This spec covers
the decoding of the raw model response into a flat list of entity objects; the prompt, the model
selection and the `score`/dedup filtering are out of scope.

## ADDED Requirements

### Requirement: NER parsing tolera arrays JSON anidados

`parse_entities_json` SHALL devolver una lista **plana** de objetos de entidad aunque el LLM
envuelva el array en uno o más arrays adicionales. Para ello SHALL aplanar recursivamente todo
valor que sea un array, conservando los objetos tal cual. El aplanado SHALL aplicarse en **las tres**
estrategias de parseo: array JSON directo, objeto con campo `entities`/`results`, y array extraído
por regex del texto.

#### Scenario: Array anidado un nivel

- **GIVEN** una respuesta de Ollama igual a `[[{"label":"Python","type":"language","score":0.9}]]`
- **WHEN** se llama a `parse_entities_json` con esa respuesta
- **THEN** SHALL devolver un `Vec` con exactamente 1 objeto de entidad
- **AND** su `label` SHALL ser `Python`

#### Scenario: Anidamiento profundo

- **GIVEN** una respuesta igual a `[[[{"label":"Rust","type":"language","score":0.9}]]]`
- **WHEN** se llama a `parse_entities_json`
- **THEN** SHALL devolver un `Vec` con exactamente 1 objeto de entidad
- **AND** su `label` SHALL ser `Rust`

#### Scenario: Array plano se conserva

- **GIVEN** una respuesta igual a `[{"label":"Docker","type":"tool","score":0.8},{"label":"Linux","type":"os","score":0.7}]`
- **WHEN** se llama a `parse_entities_json`
- **THEN** SHALL devolver un `Vec` con exactamente 2 objetos de entidad
- **AND** los labels SHALL ser `Docker` y `Linux`

#### Scenario: Objeto wrapper con array anidado

- **GIVEN** una respuesta igual a `{"entities":[[{"label":"Redis","type":"database","score":0.8}]]}`
- **WHEN** se llama a `parse_entities_json`
- **THEN** SHALL devolver un `Vec` con exactamente 1 objeto de entidad
- **AND** su `label` SHALL ser `Redis`

#### Scenario: Array anidado embebido en prosa (sigla de estrategia 3)

- **GIVEN** una respuesta con texto alrededor del array, p.ej. `Aquí van las entidades: [[{"label":"Docker","type":"tool","score":0.8}]] fin`
- **WHEN** se llama a `parse_entities_json`
- **THEN** SHALL devolver exactamente 1 objeto de entidad
- **AND** su `label` SHALL ser `Docker`

#### Scenario: Objeto wrapper con campo `results`

- **GIVEN** una respuesta igual a `{"results":[{"label":"Go","type":"language","score":0.8}]}`
- **WHEN** se llama a `parse_entities_json`
- **THEN** SHALL devolver exactamente 1 objeto de entidad
- **AND** su `label` SHALL ser `Go`

#### Scenario: Respuesta vacía o sin JSON

- **GIVEN** una respuesta vacía, o texto sin ningún array JSON
- **WHEN** se llama a `parse_entities_json`
- **THEN** SHALL devolver `Vec::new()`

### Requirement: `flatten_values` aplana recursivamente sin alterar objetos

El helper `flatten_values` SHALL tomar un `Vec<serde_json::Value>` y devolver un `Vec<serde_json::Value>`
en el que ningún elemento sea a su vez un array, aplanando recursivamente cualquier nivel de
anidamiento y preservando el orden relativo de los objetos.

#### Scenario: Aplanado de varios niveles y tipos mixtos

- **GIVEN** un `Vec` con `[Array([Array([obj_A])]), obj_B, Array([obj_C])]`
- **WHEN** se llama a `flatten_values`
- **THEN** SHALL devolver `[obj_A, obj_B, obj_C]` en ese orden
- **AND** cada objeto SHALL ser idéntico (mismo contenido JSON) al original
