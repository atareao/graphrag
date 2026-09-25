# Design: Concept Map TUI

## Context

GraphRAG almacena un grafo de conocimiento en SQLite con nodos (notas, entidades, tags) y aristas (relaciones con tipo, peso y contexto). Actualmente se puede explorar el grafo solo mediante comandos textuales (`graph`, `path`). No existe una representación visual interactiva.

Este diseño añade un TUI interactivo usando `ratatui` que permite navegar el grafo visualmente desde la terminal.

## Goals / Non-Goals

**Goals:**
- TUI interactivo que muestre nodos y aristas del grafo de conocimiento
- Navegación por teclado (flechas, vim keys) y mouse
- Selección de nodo con panel de detalles (tipo, metadata, vecinos)
- Filtro por tipo de nodo (nota/entidad/tag)
- Búsqueda de nodos por nombre
- Layout automático de nodos (force-directed)
- Colores distintivos por tipo de nodo
- Comando `graphrag map` con flags `--from`, `--depth`, `--db`

**Non-Goals:**
- Edición del grafo (solo visualización)
- Exportación a DOT/Mermaid/HTML (futuro)
- Visualización de comunidades (futuro)
- Soporte para grafos con >500 nodos (límite práctico de terminal)

## Decisions

### 1. ratatui + crossterm como stack TUI

**Decisión:** Usar `ratatui` (widget-based) con backend `crossterm`.

**Alternativas consideradas:**
- **termion:** Backend raw, no mantenido activamente. Descartado.
- **termwiz:** Muy complejo para lo que necesitamos. Descartado.
- **Sin TUI (solo ASCII):** Demasiado limitado para interacción. Descartado.

**Razón:** ratatui es el estándar de facto para TUIs en Rust. Activamente mantenido, buena documentación, widgets reutilizables (`Canvas`, `List`, `Paragraph`, `Tabs`).

### 2. Canvas widget para renderizar el grafo

**Decisión:** Usar `ratatui::widgets::Canvas` para dibujar nodos y aristas.

**Alternativas consideradas:**
- **List widget:** Solo texto, no permite posicionamiento 2D. Descartado.
- **Layout personalizado:** Demasiado complejo. Descartado.

**Razón:** Canvas permite dibujar líneas, rectángulos y texto en coordenadas 2D, que es exactamente lo que necesitamos para un grafo.

### 3. Force-directed layout

**Decisión:** Implementar algoritmo force-directed simple (Fruchterman-Reingold simplificado).

**Alternativas consideradas:**
- **Layout jerárquico:** No refleja bien la estructura de un grafo de conocimiento.
- **Layout circular:** Simple pero no muestra relaciones reales.
- **Layout fijo (grid):** No aprovecha la estructura del grafo.

**Razón:** Force-directed produce layouts orgánicos que reflejan la estructura del grafo: nodos relacionados quedan cerca, no relacionados se separan.

### 4. Arquitectura de eventos (AppState + update loop)

**Decisión:** Arquitectura clásica de TUI con un `AppState` central y un bucle event-loop.

**Razón:** Es el patrón recomendado por ratatui. Sencillo, predecible, fácil de testear.

### 5. Colores por tipo de nodo

**Decisión:** Paleta fija: note=azul, entity=verde, tag=amarillo, root=blanco, system=gris.

**Razón:** Proporciona reconocimiento visual inmediato del tipo de nodo sin necesidad de mirar el panel de detalles.

## Arquitectura

```
┌─────────────────────────────────────────────────────────────┐
│                        AppState                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌───────────┐  │
│  │  nodes   │  │  edges   │  │ selected │  │  filter   │  │
│  │  layout  │  │          │  │  node    │  │  mode     │  │
│  └──────────┘  └──────────┘  └──────────┘  └───────────┘  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                  │
│  │  search  │  │  offset  │  │  depth   │                  │
│  │  query   │  │  (pan)   │  │          │                  │
│  └──────────┘  └──────────┘  └──────────┘                  │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                      TUI Layout                             │
│  ┌──────────────────────────────────────┬──────────────────┐ │
│  │                                      │   Detail Panel   │ │
│  │         Graph Canvas                 │   ┌──────────┐   │ │
│  │                                      │   │ Node info │   │ │
│  │   [Python]────[SQLAlchemy]           │   │ Neighbors │   │ │
│  │       │                             │   │ Metadata  │   │ │
│  │       │                             │   └──────────┘   │ │
│  │   [Docker]   [PostgreSQL]            │                  │ │
│  │                                      │                  │ │
│  ├──────────────────────────────────────┴──────────────────┤ │
│  │  Status Bar: node count | filter | search | help       │ │
│  └─────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

### Flujo de datos

1. `cmd_map()` abre conexión SQLite, carga nodos y aristas
2. Ejecuta force-directed layout para posicionar nodos
3. Inicia el event-loop de ratatui
4. En cada frame:
   - Renderiza Canvas con nodos (cajas + labels) y aristas (líneas + labels)
   - Renderiza panel de detalles del nodo seleccionado
   - Renderiza barra de estado
5. En cada evento de teclado:
   - Flechas: mueve selección al vecino más cercano en esa dirección
   - Enter: selecciona nodo bajo el cursor
   - `/`: abre búsqueda
   - `t`: cicla filtro por tipo
   - `+`/`-`: cambia profundidad
   - `q`/Esc: sale

### Layout de nodos (force-directed)

Algoritmo simplificado de Fruchterman-Reingold:

```
1. Inicializar posiciones aleatorias
2. Iterar N veces (default: 50):
   a. Fuerza de repulsión entre todos los pares de nodos
   b. Fuerza de atracción a lo largo de aristas
   c. Actualizar posiciones con factor de enfriamiento
3. Normalizar al área del Canvas
```

### Carga de datos desde SQLite

```sql
-- Nodos visibles (opcionalmente desde un nodo central con profundidad)
SELECT id, label, type FROM nodes
WHERE type != 'root' AND type != 'system'
  AND (--from es opcional: si se especifica, solo nodos alcanzables)

-- Aristas entre nodos visibles
SELECT e.source_id, e.target_id, e.type, e.weight, e.context
FROM edges e
JOIN nodes ns ON ns.id = e.source_id
JOIN nodes nt ON nt.id = e.target_id
```

## Estructura de módulos

```
src/map/
├── mod.rs          — Punto de entrada: cmd_map(), carga de datos
├── tui.rs          — TUI: AppState, event-loop, renderizado
└── layout.rs       — Force-directed layout algorithm
```

### `src/map/mod.rs`

```rust
pub struct MapNode {
    pub id: i64,
    pub label: String,
    pub type_: String,
}

pub struct MapEdge {
    pub source_id: i64,
    pub target_id: i64,
    pub type_: String,
    pub weight: f64,
    pub context: Option<String>,
}

pub struct GraphData {
    pub nodes: Vec<MapNode>,
    pub edges: Vec<MapEdge>,
}

pub fn load_graph(db: &str, from: Option<&str>, depth: i32) -> Result<GraphData>;
pub fn cmd_map(db: &str, from: Option<String>, depth: i32) -> Result<()>;
```

### `src/map/layout.rs`

```rust
pub struct Position {
    pub x: f64,
    pub y: f64,
}

pub struct Layout {
    pub positions: Vec<Position>,
    pub width: f64,
    pub height: f64,
}

pub fn force_directed_layout(
    nodes: &[MapNode],
    edges: &[MapEdge],
    width: f64,
    height: f64,
    iterations: usize,
) -> Layout;
```

### `src/map/tui.rs`

```rust
pub struct AppState {
    pub nodes: Vec<MapNode>,
    pub edges: Vec<MapEdge>,
    pub positions: Vec<Position>,
    pub selected_idx: Option<usize>,
    pub filter_type: Option<String>,  // None = all, Some("note"), Some("entity"), Some("tag")
    pub search_query: String,
    pub search_active: bool,
    pub offset_x: f64,
    pub offset_y: f64,
    pub depth: i32,
}

pub fn run_tui(state: AppState) -> Result<()>;
```

## Riesgos / Trade-offs

| Riesgo | Mitigación |
|--------|------------|
| **Rendimiento con muchos nodos** (>200) | Limitar profundidad por defecto a 2. El force-directed layout escala O(n²) — añadir límite de nodos. |
| **Solapamiento de nodos en layout** | Añadir detección de colisiones y separación en el algoritmo. |
| **Terminales sin color** | ratatui maneja terminals monocromo automáticamente. |
| **Mouse no disponible** | Navegación completa por teclado como fallback. |
| **Labels largos se solapan** | Truncar labels a ~20 caracteres en el Canvas, mostrar completo en panel de detalles. |

## Open Questions

- ¿Soporte para scroll/pan cuando el grafo es más grande que la terminal? → Sí, con offset_x/offset_y.
- ¿Animar el layout inicial (mostrar el force-directed en tiempo real)? → No por ahora, sería complejo y lento.
- ¿Mostrar edge labels en el Canvas o solo en el panel de detalles? → En el Canvas si hay espacio, sino solo en detalles.