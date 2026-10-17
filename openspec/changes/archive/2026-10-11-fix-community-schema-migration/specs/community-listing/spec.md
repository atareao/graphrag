# community-listing Specification

## ADDED Requirements

### Requirement: list migra el esquema antes de leer

`cmd_community_list` SHALL ejecutar la inicialización/migración de esquema (`schema::init_db`) sobre
su conexión antes de listar, para que las consultas que seleccionan `keywords`/`topics` funcionen
sobre una base antigua. La migración SHALL ser idempotente y `community list` SHALL NOT requerir Ollama.

#### Scenario: list sobre una base v2 migra y lista

- **GIVEN** una base de datos `user_version=2` (sin columnas `keywords`/`topics`) con comunidades
- **WHEN** el usuario ejecuta `graphrag community list <db>`
- **THEN** la base SHALL quedar con `user_version=3`
- **AND** el comando SHALL listar las comunidades sin error de columna inexistente
- **AND** SHALL NOT realizar ninguna llamada a Ollama
