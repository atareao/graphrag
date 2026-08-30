# Guía de Usuario — GraphRAG

**GraphRAG** es un motor de búsqueda híbrida que combina **vectores semánticos** (embeddings) con **grafos de conocimiento** para encontrar artículos relacionados en tus notas Markdown.

Todo en un **solo binario Rust**, sin Python, sin npm, sin servidores. 100% local y privado.

---

## Índice

- [Instalación](#instalación)
- [Primeros pasos](#primeros-pasos)
- [Configuración](#configuración)
- [Comandos CLI](#comandos-cli)
- [Autocompletado para el shell](#autocompletado-para-el-shell)
- [Plugin de NeoVim](#plugin-de-neovim)
- [Servidor MCP](#servidor-mcp)
- [Ejemplos](#ejemplos)
- [Preguntas frecuentes](#preguntas-frecuentes)

---

## Instalación

### Requisitos

- **Rust ≥ 1.75** ([instalar](https://rustup.rs/))
- **Opcional:** [Ollama](https://ollama.ai/) con `nomic-embed-text` para embeddings reales y un modelo LLM para extraer entidades (ej: `llama3.2:3b`)

### Compilar

```bash
git clone <repo> graphrag
cd graphrag
cargo build --release
```

El binario se encuentra en `target/release/graphrag`. Mueve a tu `PATH`:

```bash
cp target/release/graphrag ~/.local/bin/
# o
sudo cp target/release/graphrag /usr/local/bin/
```

### Dependencias del sistema

Ninguna. SQLite se compila estáticamente dentro del binario (`bundled`).

---

## Primeros pasos

### 1. Probar con datos demo

```bash
graphrag seed graph.db
graphrag search "Python" graph.db -k 5
graphrag path "Docker" "SQLite" graph.db
graphrag stats graph.db
```

### 2. Construir grafo con tus notas

```bash
graphrag build ~/notas graph.db
```

Esto escanea recursivamente el directorio `~/notas` en busca de archivos `.md`, extrae entidades mediante Ollama y construye el grafo de conocimiento.

### 3. Buscar

```bash
graphrag search "seguridad en contenedores" graph.db -k 10 -d 2
```

---

## Configuración

### Archivo de configuración

GraphRAG crea automáticamente un archivo de configuración por defecto en:

- **Linux:** `~/.config/graphrag/config.toml`
- **XDG:** `$XDG_CONFIG_HOME/graphrag/config.toml`

La primera vez que ejecutes cualquier comando, se genera solo:

```toml
# GraphRAG Configuration
# Auto-generated. All fields are optional — CLI flags override these values.

# Default database path
db = "graph.db"

# Ollama server URL
ollama_url = "http://localhost:11434"

# Embedding model (used when --ollama is passed)
embed_model = "nomic-embed-text"

# Model for entity extraction during build
ner_model = "llama3.2:3b"

# Default number of search results
k = 5

# Default graph expansion depth
depth = 2

# Vector weight (0.0 = pure graph, 1.0 = pure vector)
alpha = 0.7

# Notes directory (used by the Neovim plugin)
notes_dir = ""
```

Puedes editar este archivo para cambiar los valores por defecto. **Los flags de CLI tienen prioridad** sobre el archivo de configuración.

### Usar un archivo de configuración alternativo

```bash
graphrag -C /ruta/mi-config.toml search "consulta" db
```

---

## Comandos CLI

### `graphrag init db <ruta>`

Inicializa una base de datos SQLite vacía con el esquema de GraphRAG.

```bash
graphrag init db graph.db
```

### `graphrag init neovim [--output <ruta>]`

Genera la configuración del plugin de NeoVim. Por defecto la imprime en stdout; usa `--output` para guardarla en un archivo.

```bash
# Mostrar en pantalla
graphrag init neovim

# Guardar en archivo
graphrag init neovim --output ~/.config/nvim/lua/plugins/graphrag.lua
```

### `graphrag build <directorio> [base_de_datos]`

Escanea un directorio de archivos `.md`, extrae entidades y construye el grafo de conocimiento.

```bash
# Build básico (embeddings sintéticos)
graphrag build ~/notas graph.db

# Build completo con Ollama
graphrag build ~/notas graph.db \
  --ollama-url http://localhost:11434 \
  --ner-model llama3.2:3b
```

**Incremental:** solo procesa archivos nuevos o modificados (hash SHA256). Archivos eliminados se podan automáticamente.

### `graphrag search <consulta> [base_de_datos]`

Búsqueda híbrida: vectores semánticos + expansión por grafo.

```bash
graphrag search "Python y bases de datos" graph.db -k 10 -d 2
```

| Flag | Default | Descripción |
|------|---------|-------------|
| `-k` | 5 | Número de resultados |
| `-d` | 2 | Profundidad de expansión en el grafo |
| `-a` | 0.7 | Peso vectorial vs grafo (0.0 = solo grafo, 1.0 = solo vectores) |
| `--vector-only` | — | Solo búsqueda vectorial, sin grafo |
| `--min-weight` | — | Peso mínimo de arista para expansión |
| `--ollama` | — | Usar Ollama para embeddings (por defecto: sintéticos) |

### `graphrag fts <consulta> [base_de_datos]`

Búsqueda textual exacta con FTS5.

```bash
graphrag fts "Python" graph.db -l 20
```

### `graphrag graph <etiqueta> [base_de_datos]`

Muestra los vecinos de un nodo en el grafo.

```bash
graphrag graph "Docker" graph.db -d 2
```

### `graphrag path <origen> <destino> [base_de_datos]`

Camino más corto entre dos nodos.

```bash
graphrag path "Python" "Docker" graph.db
```

### `graphrag stats [base_de_datos]`

Estadísticas del grafo.

```bash
graphrag stats graph.db
```

### `graphrag seed [base_de_datos]`

Puebla la base de datos con 44 nodos y 113 aristas de datos demo.

```bash
graphrag seed graph.db
```

### `graphrag mcp`

Inicia un servidor MCP sobre stdio (ver sección [Servidor MCP](#servidor-mcp)).

### `graphrag completions <shell>`

Genera scripts de autocompletado para el shell (ver [Autocompletado](#autocompletado-para-el-shell)).

---

## Autocompletado para el shell

GraphRAG puede generar scripts de autocompletado para **bash**, **zsh** y **fish**.

### Bash

```bash
graphrag completions bash > ~/.local/share/bash-completion/completions/graphrag
# o
graphrag completions bash | sudo tee /usr/share/bash-completion/completions/graphrag
```

### Zsh

```bash
graphrag completions zsh > /usr/local/share/zsh/site-functions/_graphrag
# o si usas oh-my-zsh:
graphrag completions zsh > ${ZSH_CUSTOM:-~/.oh-my-zsh/custom}/plugins/graphrag/_graphrag
```

### Fish

```bash
graphrag completions fish > ~/.config/fish/completions/graphrag.fish
```

---

## Plugin de NeoVim

### Generar la configuración

```bash
graphrag init neovim --output ~/.config/nvim/lua/plugins/graphrag.lua
```

Esto genera un archivo Lua completo listo para usar con **lazy.nvim**.

### Instalación con lazy.nvim

```lua
-- ~/.config/nvim/lua/plugins/graphrag.lua
return {
  "graphrag.nvim",
  config = function()
    require("graphrag").setup({
      db = "/home/usuario/notas/graph.db",
      bin = "/home/usuario/graphrag/target/release/graphrag",
      k = 5,
      depth = 2,
      notes_dir = "/home/usuario/notas",
    })
  end,
}
```

### Comandos disponibles

| Comando | Descripción |
|---------|-------------|
| `:GraphRAG related` | Busca notas relacionadas al buffer actual. Ventana flotante. |
| `:GraphRAG insert` | Busca relacionadas e inserta `## Relacionados` con enlaces al final. |
| `:GraphRAG search <q>` | Búsqueda híbrida |
| `:GraphRAG fts <q>` | Búsqueda textual |
| `:GraphRAG path <A> <B>` | Camino más corto |
| `:GraphRAG stats` | Estadísticas |

### Atajos de teclado

| Atajo | Comando |
|-------|---------|
| `,gr` | `:GraphRAG related` |
| `,gi` | `:GraphRAG insert` |

### Ejemplo de uso

1. Abres `docker/seguridad-en-docker.md`
2. Pulsas `,gi`
3. Se inserta al final:

```markdown
## Relacionados

- [AppArmor en Ubuntu](../seguridad/apparmor.md)
- [Hardening de imágenes Docker](hardening-docker.md)
```

Las rutas son **relativas al archivo actual**.

---

## Servidor MCP

GraphRAG incluye un servidor MCP (Model Context Protocol) que permite a asistentes de IA como **Claude Desktop** interactuar con tu grafo de conocimiento.

### Configuración en Claude Desktop

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

| Tool | Descripción |
|------|-------------|
| `build` | Construye/actualiza el grafo |
| `search` | Búsqueda híbrida |
| `fts` | Búsqueda textual |
| `graph` | Vecinos de un nodo |
| `path` | Camino más corto |
| `stats` | Estadísticas |
| `seed` | Datos demo |

### Recursos MCP

| URI | Contenido |
|-----|-----------|
| `graphrag://stats` | Estadísticas |
| `graphrag://nodes` | Todos los nodos |
| `graphrag://notes` | Solo notas |
| `graphrag://entities` | Solo entidades |
| `graphrag://edges` | Aristas |

---

## Ejemplos

### Búsqueda básica

```bash
graphrag search "Python" graph.db
```

### Búsqueda con más resultados y expansión profunda

```bash
graphrag search "seguridad en contenedores" graph.db -k 10 -d 3
```

### Búsqueda solo vectorial (sin grafo)

```bash
graphrag search "bases de datos" graph.db --vector-only
```

### Búsqueda con Ollama para embeddings reales

```bash
graphrag search "machine learning" graph.db --ollama
```

### Búsqueda textual exacta

```bash
graphrag fts "Docker" graph.db
```

### Explorar el grafo

```bash
# Vecinos de un nodo
graphrag graph "Docker" graph.db -d 2

# Camino más corto entre dos conceptos
graphrag path "Python" "Kubernetes" graph.db

# Estadísticas
graphrag stats graph.db
```

### Construir grafo desde tus notas

```bash
# Con Ollama (recomendado)
graphrag build ~/notas graph.db \
  --ollama-url http://localhost:11434 \
  --ner-model llama3.2:3b

# Sin Ollama (solo embeddings sintéticos, sin entidades)
graphrag build ~/notas graph.db
```

---

## Preguntas frecuentes

### ¿Necesito Ollama?

No para lo básico. GraphRAG usa **embeddings sintéticos** por defecto (deterministas, basados en hash). Necesitas Ollama para:

- **Extracción de entidades** durante `build` (NER)
- **Embeddings reales** durante `search` (con `--ollama`)

### ¿Qué modelos de Ollama recomiendas?

| Propósito | Modelo |
|-----------|--------|
| Embeddings | `nomic-embed-text` (384d) o `bge-m3` (1024d) |
| Extracción de entidades (NER) | `llama3.2:3b` o `gemma4:12b` |

### ¿Dónde se guarda la base de datos?

Por defecto en `graph.db` en el directorio actual. Puedes cambiarlo en el archivo de configuración o con el flag `--db`.

### ¿Es incremental?

Sí. `graphrag build` solo reprocesa archivos nuevos o modificados (detectados por hash SHA256). Los archivos eliminados se podan automáticamente.

### ¿Puedo usarlo sin conexión?

Sí. Sin Ollama, todo funciona 100% offline usando embeddings sintéticos.

### ¿Qué tamaño tiene el binario?

~6.5 MB comprimido. Es un solo binario estático sin dependencias externas.

---

## Referencia rápida

```bash
# Instalación
cargo build --release
cp target/release/graphrag ~/.local/bin/

# Demo
graphrag seed graph.db
graphrag search "Python" graph.db -k 5

# Autocompletado
graphrag completions bash > ~/.local/share/bash-completion/completions/graphrag

# Plugin NeoVim
graphrag init neovim --output ~/.config/nvim/lua/plugins/graphrag.lua

# Uso real
graphrag build ~/notas graph.db
graphrag search "tema" graph.db -k 10 -d 2
graphrag stats graph.db

# MCP para Claude Desktop
graphrag mcp --db graph.db
```