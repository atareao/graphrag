# community-detection Specification

## ADDED Requirements

### Requirement: detect migra el esquema antes de escribir

`run_community_detection` SHALL ejecutar la inicialización/migración de esquema (`schema::init_db`)
sobre su conexión **antes** de escribir en la tabla `communities`, de modo que una base de datos
antigua (p. ej. `user_version=2`, sin las columnas `keywords`/`topics`) quede migrada a la versión
actual antes de insertar. La migración SHALL ser idempotente.

#### Scenario: detect sobre una base v2 migra y no falla

- **GIVEN** una base de datos `user_version=2` cuya tabla `communities` NO tiene las columnas `keywords`/`topics`
- **AND** un grafo de entidades con aristas `co_occurs_with`
- **WHEN** `graphrag community detect <db>` ejecuta
- **THEN** la base SHALL quedar con `user_version=3`
- **AND** las comunidades detectadas SHALL insertarse sin error
- **AND** no SHALL aparecer ningún error de columna inexistente

### Requirement: detect reemplaza comunidades de forma atómica

El reemplazo de comunidades en `run_community_detection` (borrado de las comunidades previas e
inserción de las nuevas) SHALL ser **atómico**. Si cualquier inserción falla, el borrado SHALL
revertirse y las comunidades preexistentes SHALL conservarse, de forma que la tabla nunca quede
vacía por un fallo a mitad de operación.

#### Scenario: fallo de inserción preserva las comunidades existentes

- **GIVEN** una base de datos con N comunidades ya almacenadas (N > 0)
- **AND** una condición que hace fallar la inserción de las nuevas comunidades (p. ej. esquema incompatible)
- **WHEN** `run_community_detection` ejecuta
- **THEN** la operación SHALL devolver error
- **AND** las N comunidades originales SHALL permanecer intactas
- **AND** la tabla `communities` SHALL NOT quedar vacía

#### Scenario: detección sin comunidades no borra las existentes

- **GIVEN** una base de datos con M comunidades existentes
- **AND** un grafo que produce 0 comunidades nuevas
- **WHEN** `run_community_detection` ejecuta
- **THEN** SHALL completarse sin error
- **AND** el resultado SHALL ser determinista y consistente con el borrado+inserción transaccional
