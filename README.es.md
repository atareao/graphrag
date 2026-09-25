# graphrag

**Motor de búsqueda híbrido: embeddings vectoriales + grafo de conocimiento sobre notas Markdown.**

GraphRAG escanea un repositorio de notas Markdown, extrae entidades mediante IA local (Ollama), construye un grafo de conocimiento y permite búsqueda híbrida (vectores semánticos + expansión por grafo) para encontrar artículos relacionados.

Todo en **un único binario Rust** — sin Python, sin npm, sin servidores. 100% local y privado.

[![Crates.io](https://img.shields.io/crates/v/graphrag.svg)](https://crates.io/crates/graphrag)
[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Inicio rápido

```bash
# 1. Instalar
cargo install graphrag

# 2. Poblar datos de demo y probar
graphrag seed graph.db
graphrag search "Python" graph.db -k 5
graphrag path "Docker" "SQLite" graph.db
graphrag stats graph.db

# 3. Construir grafo desde tus notas (usa ~/.config/graphrag/config.toml)
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
cargo install graphrag
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

**Opcional:** [Ollama](https://ollama.ai/) con un modelo de embeddings (ej. `bge-m3`) y un LLM para extracción de entidades (ej. `gemma4:e2b`).

---

## Configuración

GraphRAG crea automáticamente un archivo de configuración en `~/.config/graphrag/config.toml` al primer inicio:

```toml
# Ruta por defecto de la base de datos
db = "graph.db"

# URL del servidor Ollama
ollama_url = "http://localhost:11434"

# Modelo de embeddings (usado cuando se pasa --ollama)
embed_model = "bge-m3:latest"

# Modelo para extracción de entidades durante build
ner_model = "gemma4:e2b"

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

### `graphrag completions <bash|zsh|fish>`

Genera scripts de autocompletado para la shell.

```bash
graphrag completions bash > completions.bash
graphrag completions zsh > _graphrag
graphrag completions fish > graphrag.fish
```

### `graphrag seed [DB]`

Puebla la base de datos con datos de demo (44 nodos, ~113 aristas).

```bash
graphrag seed graph.db
graphrag search "Docker security" graph.db -k 5
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
graphrag build ~/notes graph.db --ner-model gemma4:e2b
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
| `-d` | 2 | Profundidad de expansión en el grafo |
| `-a` | 0.7 | Peso vectorial (0.0 = solo grafo, 1.0 = solo vectores) |
| `--vector-only` | — | Búsqueda solo vectorial, sin grafo |
| `--notes-only` | — | Mostrar solo notas (sin entidades/tags) |
| `--min-weight` | — | Peso mínimo de arista para expansión |
| `--ollama` | — | Usar Ollama para embeddings (por defecto: synthetic) |

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

Nota: `--label` y `--file` son mutuamente excluyentes. Usa exactamente uno.

### `graphrag fts <CONSULTA> [DB]`

Búsqueda exacta de texto con FTS5.

```bash
graphrag fts "Python" graph.db -l 20
```

### `graphrag graph <ETIQUETA> [DB]`

Muestra los vecinos de un nodo en el grafo.

```bash
graphrag graph "Docker" graph.db -d 2
```

### `graphrag path <DESDE> <HASTA> [DB]`

Camino más corto entre dos nodos.

```bash
graphrag path "Python" "Docker" graph.db
```

### `graphrag stats [DB]`

Estadísticas del grafo.

```bash
graphrag stats graph.db
```

### `graphrag mcp`

Inicia un servidor MCP sobre stdio (ver sección MCP).

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
│   ├── main.rs           ← CLI (clap, 11 subcomandos)
│   ├── config.rs         ← Config TOML (auto-creada)
│   ├── mcp/mod.rs        ← Servidor MCP (JSON-RPC stdio)
│   ├── db/schema.rs      ← Esquema SQLite + FTS5
│   ├── graph/
│   │   ├── build.rs      ← Construcción incremental + paralela
│   │   └── expand.rs     ← CTE recursivo, camino más corto
│   ├── search/hybrid.rs  ← HybridSearch 3 fases
│   ├── embed/ollama.rs   ← Cliente HTTP Ollama
│   ├── vector/
│   │   └── mod.rs        ← Blob <-> Vec<f32>
│   ├── chunking/markdown.rs ← Parseo de frontmatter + chunking
│   ├── ner/ollama_ner.rs ← Extracción de entidades (modo lote)
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
Consulta → FASE 1: Similitud coseno sobre embeddings
         → FASE 2: Reordenar por título/tipo
         → FASE 3: CTE recursivo de expansión en grafo
         → Resultados con puntuación, contenido, ruta y vecinos
```

---

## Rendimiento

### Modelos recomendados

| Propósito | Modelo | Parámetros | VRAM | Calidad |
|-----------|--------|-------------|------|---------|
| NER | `gemma4:e2b` | 5.1B | ~5GB | Muy buena |
| NER | `gemma4:12b` | 11.9B | ~7GB | Excelente |
| NER | `llama3.2:3b` | 3.2B | ~2GB | Buena (rápida) |
| Embeddings | `bge-m3` | 566M | ~1GB | Excelente |
| Embeddings | `nomic-embed-text` | 137M | ~0.5GB | Buena |

### Rendimiento

Con `gemma4:e2b` y 4 hilos: ~0.08 archivos/s → ~22h para 6254 archivos (solo la primera construcción; las siguientes son incrementales).

### Configuración

```toml
# ~/.config/graphrag/config.toml
ner_model = "gemma4:e2b"
num_threads = 4
```

---

## Tests

```bash
cargo test                    # 91 tests, 12 ignorados (necesitan Ollama)
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