# Proposal: Concept Map TUI

## Why

GraphRAG almacena un grafo de conocimiento completo (nodos + aristas) pero solo puede mostrarlo como texto plano (`graph` y `path`). No hay forma de **explorar visualmente** el grafo desde la terminal. Un TUI interactivo permitiría navegar el grafo, inspeccionar nodos y descubrir relaciones de forma visual e inmediata, sin depender de herramientas externas (navegador, Graphviz).

## What Changes

- **Nuevo comando `graphrag map`** que abre un TUI interactivo para explorar el grafo de conocimiento
- **Nuevo módulo `src/map/`** con:
  - `mod.rs` — entrada del módulo, orquestación
  - `tui.rs` — TUI interactivo con ratatui
  - `layout.rs` — algoritmo force-directed para posicionar nodos
- **Dependencia nueva**: `ratatui` + `crossterm`
- El comando `graphrag map` acepta:
  - `--from <label>` — nodo central opcional
  - `-d, --depth <n>` — profundidad de expansión (default: 2)
  - `--db <path>` — base de datos (default: config)
- El TUI ofrece:
  - Visualización de nodos como cajas con etiqueta
  - Aristas como líneas con flechas y etiquetas de relación
  - Navegación con teclado (flechas / vim keys)
  - Selección de nodo con detalles en panel lateral/inferior
  - Filtro por tipo de nodo (nota, entidad, tag)
  - Búsqueda de nodos por nombre
  - Colores por tipo de nodo
  - Soporte de mouse (click en nodos)

## Capabilities

### New Capabilities
- `concept-map-tui`: Interactive terminal-based concept map explorer for the knowledge graph

### Modified Capabilities
- *(none — this is a new capability, no existing specs change)*

## Impact

- **Nuevo módulo**: `src/map/` (~600-800 líneas total)
- **Dependencias**: `ratatui` + `crossterm` en `Cargo.toml`
- **CLI**: Nuevo subcomando `Map` en `Commands` enum
- **MCP**: Opcionalmente añadir herramienta `concept_map` que devuelva DOT para visualización externa
- **Sin cambios** en módulos existentes (solo se lee de la BD, no se modifica)