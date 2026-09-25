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