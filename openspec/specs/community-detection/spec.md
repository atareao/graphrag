# community-detection Specification

## Purpose
Runs Leiden community detection on the entity graph (entities + `co_occurs_with` edges) to identify densely-connected clusters (communities) at hierarchical levels 1 and 2, and stores the partitions in the `communities` table.

## Requirements

### Requirement: Entity graph is loaded from SQLite
`detect` SHALL load the entity graph from the database by querying:
- All nodes with type NOT IN (`'note'`, `'tag'`) — these are the entity nodes for community detection
- All edges with type `'co_occurs_with'` with their weights — these form the relationships

#### Scenario: Load entity graph from seeded database
- **GIVEN** a database with 10 entities (tools, languages) and 15 `co_occurs_with` edges
- **WHEN** `load_entity_graph()` is called
- **THEN** it SHALL return a list of entity nodes with their IDs and labels
- **AND** a list of edges with source_id, target_id, and weight

#### Scenario: Notes and tags are excluded
- **GIVEN** a database with entities, notes, and tags
- **WHEN** `load_entity_graph()` is called
- **THEN** nodes with type `'note'` or `'tag'` SHALL NOT be included in the entity graph

### Requirement: Leiden algorithm runs with CPM quality function
`detect` SHALL run the Leiden algorithm using `leiden-rs` with the CPM (Constant Potts Model) quality function and configurable resolution parameter (default 1.0). The graph SHALL be treated as undirected with weighted edges.

#### Scenario: Leiden produces hierarchical communities
- **GIVEN** an entity graph with 10 entities and known clusters (e.g., Python+FastAPI+SQLite as one cluster, Rust+Actix+Diesel as another)
- **WHEN** `run_leiden()` is called with resolution=1.0
- **THEN** it SHALL return a `HierarchicalOutput` with at least 2 levels
- **AND** entities in the same expected cluster SHALL appear in the same community

### Requirement: Communities stored at levels 1 and 2
`detect` SHALL insert Level 1 and Level 2 communities into the `communities` table. Level 1 communities SHALL have `parent_id = NULL`. Level 2 communities SHALL reference their parent Level 1 community via `parent_id`. Communities with too few members (less than 2 entities) SHALL be discarded.

#### Scenario: Level 1 communities stored
- **WHEN** `store_communities()` is called after successful detection
- **THEN** each Level 1 community SHALL have a row in `communities` with `level = 1` and `parent_id = NULL`
- **AND** `member_ids` SHALL be a JSON array of entity node IDs
- **AND** `member_count` SHALL equal the number of member entities

#### Scenario: Level 2 communities reference parent
- **WHEN** `store_communities()` stores Level 2 communities
- **THEN** each Level 2 community SHALL have `level = 2`
- **AND** `parent_id` SHALL reference the ID of the containing Level 1 community
- **AND** all members of a Level 2 community SHALL be a subset of the parent Level 1 community's members

#### Scenario: Singleton communities discarded
- **WHEN** a community (at any level) has only 1 member entity
- **THEN** it SHALL NOT be stored in the `communities` table

### Requirement: CLI integration
`graphrag community detect <db>` SHALL be a valid command that runs the full pipeline (load → run Leiden → store). It SHALL accept an optional `--resolution` flag (default 1.0) and display community statistics after completion.

#### Scenario: Community detect from CLI
- **GIVEN** a seeded database
- **WHEN** `graphrag community detect test.db` runs
- **THEN** it SHALL display the number of communities found at each level
- **AND** exit with status 0

#### Scenario: Empty database
- **GIVEN** a database with no entities
- **WHEN** `graphrag community detect test.db` runs
- **THEN** it SHALL display "No entities found for community detection"
- **AND** exit with status 0

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
