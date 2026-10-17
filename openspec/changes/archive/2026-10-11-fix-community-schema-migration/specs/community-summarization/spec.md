# community-summarization Specification

## ADDED Requirements

### Requirement: summarize migra el esquema antes de escribir

`summarize_all_communities` SHALL ejecutar la inicialización/migración de esquema (`schema::init_db`)
sobre su conexión antes de actualizar filas de `communities`, para poder persistir `keywords`/`topics`
en una base antigua. La migración SHALL ser idempotente.

#### Scenario: summarize sobre una base v2 migra y persiste

- **GIVEN** una base de datos `user_version=2` (sin columnas `keywords`/`topics`) con al menos una comunidad y Ollama disponible
- **WHEN** `graphrag community summarize <db>` ejecuta
- **THEN** la base SHALL quedar con `user_version=3`
- **AND** la comunidad resumida SHALL tener `keywords`/`topics` persistidos sin error de esquema
