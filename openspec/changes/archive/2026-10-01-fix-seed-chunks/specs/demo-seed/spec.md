# demo-seed Specification

## Purpose

`graphrag seed` builds a self-contained demo database that is immediately searchable: it stores the demo notes, entities and edges, and additionally embeds each note's content into the `chunks` table so that `search`, `ask` and `similar` return results on a freshly seeded database without running `build`.

## ADDED Requirements

### Requirement: `seed` inserta chunk embeddings buscables

`graphrag seed` SHALL insertar al menos un chunk por nota demo en la tabla `chunks`, con `header` igual al título de la nota, `text` igual al contenido de la nota, `slug` igual al slug del título, y un `embedding` no nulo calculado vía Ollama a partir del contenido de la nota. La inserción SHALL ocurrir dentro de la misma transacción que inserta nodos y aristas, y antes del repoblado de FTS5. De este modo `search`, `ask` y `similar --label` SHALL devolver resultados sobre la BD demo.

#### Scenario: Demo search devuelve resultados

- **GIVEN** una BD sembrada con `seed` y Ollama disponible
- **WHEN** el usuario ejecuta `search "<tema>" db -k 5`
- **THEN** se devuelve ≥1 resultado
- **AND** el resultado incluye una nota y su score

#### Scenario: stats refleja chunks

- **GIVEN** la BD demo
- **WHEN** el usuario ejecuta `stats db`
- **THEN** `Chunks` SHALL ser > 0
- **AND** `Con embeddings` SHALL ser > 0

#### Scenario: similar --label funciona sobre la demo

- **GIVEN** la BD demo
- **WHEN** el usuario ejecuta `similar --label "<nota>" db`
- **THEN** se devuelve ≥1 nota similar

### Requirement: `seed` requiere Ollama y es atómico

`seed` SHALL requerir Ollama para calcular los embeddings de nodos y chunks. Si Ollama no está accesible, SHALL fallar con un error de conexión y SHALL NOT dejar una base de datos parcialmente poblada.

#### Scenario: Ollama caído

- **GIVEN** Ollama no disponible
- **WHEN** el usuario ejecuta `seed db`
- **THEN** el comando SHALL salir con código no-cero
- **AND** SHALL imprimir "Error connecting to Ollama"
- **AND** la BD SHALL NOT quedar con datos parciales

### Requirement: identidad y FTS se mantienen

Los nodos SHALL seguir identificándose por `key` (`note:<slug>` para notas y `node:<label>` para el resto), y FTS5 SHALL repoblarse con las 20 notas demo tras el seed.

#### Scenario: FTS intacto

- **GIVEN** la BD demo
- **WHEN** el usuario ejecuta `fts "<palabra>" db`
- **THEN** se devuelven notas
