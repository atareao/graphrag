# graphrag

**Motor de búsqueda híbrido: embeddings vectoriales + grafo de conocimiento sobre notas Markdown.**

GraphRAG escanea un repositorio de notas Markdown, extrae entidades mediante IA local (Ollama), construye un grafo de conocimiento y permite búsqueda híbrida (vectores semánticos + expansión por grafo) para encontrar artículos relacionados.

Todo en **un único binario Rust** — sin Python, sin npm, sin servidores. 100% local y privado.

[![Crates.io](https://img.shields.io/crates/v/graphrag.svg)](https://crates.io/crates/graphrag)
[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Inicio rápido

```bash
# 1. Instalar
cargo install graphrag-search

# 2. Poblar datos de demo (requiere Ollama) y explorarla
graphrag seed graph.db
graphrag search "Python" graph.db -k 5
graphrag fts "Python" graph.db
graphrag graph "Docker" graph.db
graphrag path "Docker" "SQLite" graph.db
graphrag stats graph.db

# 3. Construir grafo desde tus notas (requiere Ollama; usa ~/.config/graphrag/config.toml)
graphrag build

# 4. Resetear base de datos (rm + init)
graphrag reset

# 5. Generar autocompletado para shell
graphrag completions bash > ~/.local/share/bash-completion/completions/graphrag

# 6. Generar configuración del plugin para Neovim
graphrag init neovim --output ~/.config/nvim/lua/plugins/graphrag.lua

# 7. Iniciar servidor MCP para Claude Desktop
graphrag mcp --db graph.db
```

> La búsqueda híbrida (`search`, `ask`, `similar`) necesita Ollama **en ejecución**. La base de datos demo es **directamente buscable**: `seed` inserta un chunk con embedding por nota, por lo que `search`, `ask` y `similar --label` devuelven resultados sobre ella. Para buscar tus propias notas, construye un grafo con `build`.

---

## Índice

- [Instalación](#instalación)
- [Configuración](#configuración)
- [Comandos CLI](#comandos-cli)
- [Plugin Neovim](#plugin-neovim)
- [Servidor MCP](#servidor-mcp)
- [Arquitectura](#arquitectura)
- [Rendimiento](#rendimiento)
- [Tests](#tests)
- [Depuración](#depuración)

---

## Instalación

### Desde crates.io

```bash
cargo install graphrag-search
```

### Desde fuente

```bash
git clone https://github.com/atareao/graphrag
cd graphrag
cargo build --release

# Binario en:
#   target/release/graphrag (~6.5 MB)
```

**Dependencias:** Solo Rust ≥ 1.75. SQLite está compilado estáticamente (bundled).

**Ollama** es necesario para todo lo que usa vectores o LLM (embeddings, extracción de entidades y resúmenes). Descarga un modelo de embeddings (ej. `bge-m3`) y un LLM para extracción de entidades (ej. `llama3.2:3b`). Los comandos que solo leen datos ya almacenados funcionan sin él.

---

## Configuración

GraphRAG crea automáticamente un archivo de configuración en `~/.config/graphrag/config.toml` al primer inicio:

```toml
# Ruta por defecto de la base de datos
db = "graph.db"

# URL del servidor Ollama
ollama_url = "http://localhost:11434"

# Modelo de embeddings
embed_model = "bge-m3:latest"

# Modelo para extracción de entidades durante build
ner_model = "llama3.2:3b"

# Modelo para resumir comunidades (y para ask si no se pasa --model)
summary_model = "llama3.2:3b"

# Número por defecto de resultados de búsqueda
k = 5

# Profundidad de expansión por defecto en el grafo
depth = 2

# Peso vectorial (0.0 = solo grafo, 1.0 = solo vectores)
alpha = 0.7

# Directorio de notas (usado por build y el plugin Neovim)
notes_dir = ""

# Número de hilos paralelos para build (workers NER)
num_threads = 4
```

Todos los campos son opcionales. Las flags de CLI sobrescriben los valores de configuración.

---

## Comandos CLI

```bash
graphrag <COMANDO> [ARGUMENTOS]
```

### Comandos principales

| Comando | Descripción |
|---------|-------------|
| [`build`](#graphrag-build-repo-db) | 📥 Escanear archivos `.md` → extraer entidades → construir grafo |
| [`search`](#graphrag-search-consulta-db) | 🔍 Búsqueda híbrida (vectores + grafo) |
| [`ask`](#graphrag-ask-consulta-db) | 💡 RAG: responde preguntas con fuentes (retrieval + LLM) |
| [`fts`](#graphrag-fts-consulta-db) | 📄 Búsqueda exacta FTS5 |
| [`graph`](#graphrag-graph-etiqueta-db) | 🕸️ Mostrar vecinos de un nodo |
| [`map`](#graphrag-map-db) | 🖥️ Mapa conceptual interactivo TUI |
| [`path`](#graphrag-path-desde-hasta-db) | 🔗 Camino más corto entre dos nodos |
| [`seed`](#graphrag-seed-db) | 🌱 Poblar datos de demo |
| [`stats`](#graphrag-stats-db) | 📊 Estadísticas del grafo |
| [`reset`](#graphrag-reset-db) | 🗑️ Eliminar BD y recrear vacía |

### `graphrag init db [DB]`

Crea una base de datos SQLite vacía con el esquema de GraphRAG.

```bash
graphrag init db graph.db
```

### `graphrag init neovim [--output <ruta>]`

Genera la configuración del plugin Neovim lista para lazy.nvim.

```bash
# Mostrar en stdout
graphrag init neovim

# Guardar a archivo
graphrag init neovim --output ~/.config/nvim/lua/plugins/graphrag.lua
```

### `graphrag completions <bash|elvish|fish|powershell|zsh>`

Genera scripts de autocompletado para la shell.

```bash
graphrag completions bash > completions.bash
graphrag completions zsh > _graphrag
graphrag completions fish > graphrag.fish
graphrag completions elvish > graphrag.elv
graphrag completions powershell > graphrag.ps1
```

### `graphrag seed [DB]`

Puebla la base de datos con datos de demo (44 nodos, ~113 aristas). **Requiere Ollama**: genera las embeddings de nodos y de chunks con él. Si Ollama no está en marcha, el comando falla, no deja ningún fichero de BD y sale con código no-cero.

`seed` inserta un chunk (con embedding) por cada nota demo, de modo que la BD demo es **directamente buscable**: `search`, `ask` y `similar --label` devuelven resultados sobre ella mientras Ollama esté en marcha. También puedes usar `fts`, `graph`, `path`, `stats` y `map`.

```bash
graphrag seed graph.db
graphrag fts "Docker security" graph.db
```

### `graphrag build [REPO] [DB]`

Escanea un directorio de archivos `.md`, extrae entidades mediante Ollama NER y construye el grafo de conocimiento.

**Procesamiento optimizado:**
- **Lote NER:** todos los chunks de un archivo se envían en una sola llamada a Ollama
- **N hilos workers** para NER paralelo + **1 hilo writer** (SQLite serializado, sin contención de bloqueos)
- **Reintento con backoff:** 3 intentos (2s, 4s, 8s) en errores de conexión
- **Incremental:** solo procesa archivos nuevos/modificados (hash SHA256)
- **Limpieza automática:** las notas eliminadas se podan del grafo

```bash
# Construir desde el directorio de notas configurado
graphrag build

# Construir desde un directorio específico
graphrag build ~/notes graph.db

# Construir con un modelo específico
graphrag build ~/notes graph.db --ner-model llama3.2:3b
```

### `graphrag reset [DB]`

Elimina la base de datos y la recrea vacía (rm + init).

```bash
graphrag reset
graphrag reset /data/notes/graph.db
```

### `graphrag search <CONSULTA> [DB]`

Búsqueda híbrida: vectores semánticos + expansión por grafo.

```bash
graphrag search "Python and databases" graph.db -k 10 -d 2
```

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `-k` | 5 | Número de resultados |
| `-d` | 2 | Profundidad de expansión en el grafo (0 = solo vectorial) |
| `-a` | 0.7 | Peso vectorial (0.0 = solo grafo, 1.0 = solo vectores) |
| `--notes-only` | — | Mostrar solo notas (sin entidades/tags) |
| `--filter` | — | Filtro de metadatos (repetible), p. ej. `'date >= 2024'` |
| `--answer` | — | Respuesta narrativa con contexto de comunidades |
| `--min-weight` | — | Peso mínimo de arista para expansión |
| `--format` | `table` | Formato de salida: `table`, `list` o `json` |
| `--ollama-url` | `http://localhost:11434` | URL del servidor Ollama |
| `--embed-model` | `nomic-embed-text` | Modelo de embeddings |

No existe `--vector-only` ni `--ollama`: para búsqueda solo vectorial usa `-d 0`, y la conexión a Ollama se controla con `--ollama-url`. `search` **necesita Ollama en ejecución** para embeber la consulta y una BD con chunks: la demo de `seed` ya los incluye, y para tus notas necesitas un grafo construido con `build`.

### `graphrag ask <CONSULTA> [DB]`

RAG en un solo comando: retrieval híbrido + resúmenes de comunidades + evidencia de chunks → una respuesta en lenguaje natural fundamentada y generada con Ollama, con fuentes citadas.

```bash
graphrag ask "¿cómo configuro nginx?" notes.db
graphrag ask "backups" notes.db -k 10 --filter 'date >= 2024'
graphrag ask "nginx" notes.db --format json
graphrag ask "nginx" notes.db --model gpt-oss:latest --communities 5
```

| Flag | Descripción |
|:---|---|
| `-k, --k` | Resultados de retrieval usados como evidencia (por defecto 5) |
| `-d, --depth` | Profundidad de expansión en el grafo (por defecto 2; 0 = solo vectorial) |
| `-a, --alpha` | Peso vectorial, 0.0–1.0 (por defecto 0.7) |
| `--min-weight` | Peso mínimo de arista para la expansión |
| `--notes-only` | Solo notas (oculta entidades y tags) |
| `--filter` | Filtro de metadatos (repetible), p. ej. `'date >= 2023'` |
| `--communities` | Máximo de resúmenes de comunidades en el contexto (por defecto 3) |
| `--model` | Modelo de Ollama para la generación (por defecto: `summary_model` de la config) |
| `--format` | Formato de salida: `table` \| `list` \| `json` |

Si Ollama no responde, `ask` degrada a retrieval puro y sale con código 0. El contexto de comunidades requiere `graphrag community detect` seguido de `graphrag community summarize`. Con `--format json` la salida es `{ "answer", "sources", "results" }`.

### `graphrag similar --label <ETIQUETA> --file <RUTA> [DB]`

Encuentra notas semánticamente similares. Dos modos:
- `--label`: encuentra notas similares a una nota **ya indexada** (sin Ollama — usa embeddings almacenados)
- `--file`: encuentra notas similares a un archivo `.md` **externo** (chunks y embeddings vía Ollama)

Ambos modos usan **match individual**: cada chunk del origen se compara independientemente, y cada nota destino se puntúa por su mejor par de chunks.

```bash
# Encontrar notas similares a una nota existente
$ graphrag similar --label "Python frameworks" graph.db -k 5

# Encontrar notas similares a un archivo externo
$ graphrag similar --file ~/borradores/nueva-idea.md graph.db -k 10 -d 2
```

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `--label` | — | Etiqueta de una nota existente en la base de datos |
| `--file` | — | Ruta a un archivo `.md` externo |
| `-k` | 5 | Número de resultados |
| `-d` | 2 | Profundidad de expansión en el grafo |
| `--notes-only` | — | Mostrar solo notas (sin entidades/tags) |
| `--min-weight` | — | Peso mínimo de arista para expansión |
| `--filter` | — | Filtrar por campo de metadatos (`--filter "category = tutorial"`) |
| `--format` | `table` | Formato de salida: `table`, `list` o `json` |

Nota: `--label` y `--file` son mutuamente excluyentes. Usa exactamente uno.

### `graphrag fts <CONSULTA> [DB]`

Búsqueda exacta de texto con FTS5.

```bash
graphrag fts "Python" graph.db -l 20
```

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `-l` | 10 | Número de resultados |
| `--notes-only` | — | Mostrar solo notas (sin entidades/tags) |
| `--format` | `table` | Formato de salida: `table`, `list` o `json` |

### `graphrag graph <ETIQUETA> [DB]`

Muestra los vecinos de un nodo en el grafo.

```bash
graphrag graph "Docker" graph.db -d 2
```

### `graphrag map [DB]`

Mapa conceptual interactivo TUI — explora visualmente el grafo de conocimiento en la terminal.

```bash
# Grafo completo
graphrag map graph.db

# Subgrafo centrado en un nodo
graphrag map --from Python --depth 3 graph.db
```

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `--from` | — | Nodo central (omitir para grafo completo) |
| `-d` | 2 | Profundidad de expansión desde el nodo central |

**Controles:**

| Tecla | Acción |
|-------|--------|
| `↑↓←→` / `hjkl` | Navegar entre nodos |
| `Enter` | Seleccionar nodo (detalles en panel derecho) |
| `t` | Ciclar filtro: todo → notas → entidades → tags |
| `/` | Buscar nodos por nombre |
| `q` / `Esc` | Salir |
| Click mouse | Seleccionar nodo |

### `graphrag path <DESDE> <HASTA> [DB]`

Camino más corto entre dos nodos.

```bash
graphrag path "Python" "Docker" graph.db
graphrag path "Python" "Docker" graph.db -m 5
```

| Flag | Por defecto | Descripción |
|------|-------------|-------------|
| `-m, --max-depth` | 10 | Profundidad máxima de recorrido |

> La opción es **`-m`**, no `-d`.

### `graphrag stats [DB]`

Estadísticas del grafo.

```bash
graphrag stats graph.db
```

### `graphrag mcp --db <DB>`

Inicia un servidor MCP sobre stdio (ver sección MCP). La base de datos se indica con `--db`, no como argumento posicional.

---

## Plugin Neovim

El plugin `graphrag.nvim` te permite buscar artículos relacionados desde Neovim y abrir resultados o insertarlos como enlaces al final del artículo.

### Instalación

```bash
graphrag init neovim --output ~/.config/nvim/lua/plugins/graphrag.lua
```

lazy.nvim lo carga automáticamente desde `lua/plugins/`.

### Comandos

| Comando | Descripción |
|---------|-------------|
| `:GraphRAG related` | Encuentra notas relacionadas al buffer actual. Ventana flotante. |
| `:GraphRAG insert` | Encuentra relacionadas e inserta enlaces `## Related` al final. |
| `:GraphRAG search <q>` | Búsqueda híbrida |
| `:GraphRAG fts <q>` | Búsqueda textual |
| `:GraphRAG path <A> <B>` | Camino más corto |
| `:GraphRAG stats` | Estadísticas |

### Atajos de teclado

| Tecla | Comando |
|-------|---------|
| `,gr` | `:GraphRAG related` |
| `,gi` | `:GraphRAG insert` |

---

## Servidor MCP

### Configuración para Claude Desktop

```json
{
  "mcpServers": {
    "graphrag": {
      "command": "/ruta/completa/graphrag",
      "args": ["mcp", "--db", "/ruta/completa/graph.db"]
    }
  }
}
```

### Herramientas MCP

| Herramienta | Descripción |
|-------------|-------------|
| `build` | Construir/actualizar el grafo |
| `search` | Búsqueda híbrida |
| `fts` | Búsqueda textual |
| `graph` | Vecinos de un nodo |
| `path` | Camino más corto |
| `stats` | Estadísticas |
| `seed` | Datos de demo |

### Recursos MCP

| URI | Contenido |
|-----|-----------|
| `graphrag://stats` | Estadísticas |
| `graphrag://nodes` | Todos los nodos |
| `graphrag://notes` | Solo notas |
| `graphrag://entities` | Solo entidades |
| `graphrag://edges` | Aristas |

---

## Arquitectura

```
📦 graphrag
├── src/
│   ├── main.rs           ← CLI (clap)
│   ├── config.rs         ← Config TOML (auto-creada)
│   ├── db/
│   │   ├── schema.rs     ← Esquema SQLite + FTS5
│   │   ├── keys.rs       ← Identidad de nodos (note:<ruta> / node:<label>)
│   │   ├── chunks.rs     ← Chunks y embeddings de chunk
│   │   └── communities.rs ← Comunidades detectadas y resúmenes
│   ├── graph/
│   │   ├── build.rs      ← Construcción incremental + paralela
│   │   └── expand.rs     ← CTE recursivo, camino más corto
│   ├── search/
│   │   ├── hybrid.rs     ← HybridSearch 3 fases
│   │   └── filter.rs     ← Filtros de metadatos
│   ├── community/
│   │   ├── detect.rs     ← Detección de comunidades
│   │   ├── summarize.rs  ← Resumen LLM por comunidad
│   │   └── search.rs     ← Respuestas RAG
│   ├── embed/ollama.rs   ← Cliente HTTP Ollama
│   ├── vector/mod.rs     ← Blob <-> Vec<f32>, similitud coseno
│   ├── chunking/markdown.rs ← Parseo de frontmatter + chunking
│   ├── ner/ollama_ner.rs ← Extracción de entidades (modo lote)
│   ├── map/
│   │   ├── mod.rs        ← Carga de datos (load_graph, cmd_map)
│   │   ├── layout.rs     ← Algoritmo de layout force-directed
│   │   └── tui.rs        ← TUI interactivo (ratatui)
│   ├── mcp/mod.rs        ← Servidor MCP (JSON-RPC stdio)
│   └── seed/demo_data.rs ← Generador de datos de demo
└── target/release/graphrag  ← Binario único (~6.5 MB)
```

### Pipeline de construcción

```
.md → Pre-escaneo (hash SHA256) → Filtrar sin cambios
     → N workers: parsear frontmatter → chunk → lote NER (Ollama)
     → 1 writer: insertar nodos/aristas en SQLite (serializado)
     → Podar notas eliminadas → Repoblar FTS5
```

### Pipeline de búsqueda

```
Consulta → FASE 1: Similitud coseno sobre embeddings de chunk (Ollama)
         → FASE 2: Reordenar por título/tipo
         → FASE 3: CTE recursivo de expansión en grafo
         → Resultados con puntuación, contenido, ruta y vecinos
```

---

## Rendimiento

### Modelos recomendados

| Propósito | Modelo | Tamaño | Calidad |
|-----------|--------|--------|---------|
| NER | `llama3.2:3b` | ~2GB | Buena (rápida) |
| NER | `gemma4:e2b` | ~5GB | Muy buena |
| Embeddings | `bge-m3` | ~1GB | Excelente |
| Embeddings | `nomic-embed-text` | ~0.5GB | Buena |

### Rendimiento

Con `llama3.2:3b` y 4 hilos: ~0.08 archivos/s → ~22h para 6254 archivos (solo la primera construcción; las siguientes son incrementales).

### Configuración

```toml
# ~/.config/graphrag/config.toml
ner_model = "llama3.2:3b"
num_threads = 4
```

---

## Tests

```bash
cargo test                    # 143 tests, 15 ignorados (necesitan Ollama)
cargo test -- --ignored       # Tests que dependen de Ollama
```

Los tests usan `tempfile` para bases de datos temporales. No se necesitan servicios externos para los tests no ignorados.

---

## Depuración

```bash
# Logs detallados
RUST_LOG=debug graphrag build

# Filtrar por módulo
RUST_LOG=graphrag::graph::build=debug graphrag build
RUST_LOG=graphrag::ner=debug graphrag build

# Info solamente (por defecto)
graphrag build
```

---

## Licencia

MIT
