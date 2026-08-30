//! Servidor MCP (Model Context Protocol) para GraphRAG.
//!
//! Permite que asistentes de IA como Claude interactúen con el grafo
//! de conocimiento a través de herramientas (tools) y recursos (resources)
//! MCP sobre stdio.
//!
//! # Uso
//!
//! ```bash
//! graphrag mcp --db /ruta/notas.db --ollama-url http://localhost:11434
//! ```
//!
//! Luego se configura en claude_desktop_config.json como servidor MCP.

use std::io::{self, BufRead, Write};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use log::{info, error};

use crate::graph;
use crate::search::HybridSearch;

// ---------------------------------------------------------------------------
// Constantes del protocolo
// ---------------------------------------------------------------------------

/// Versión del protocolo MCP que soportamos
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

// ---------------------------------------------------------------------------
// Tipos JSON-RPC
// ---------------------------------------------------------------------------

/// Petición JSON-RPC 2.0 entrante
#[derive(Deserialize)]
struct JsonRpcRequest {
    method: String,
    params: Option<Value>,
    #[serde(default)]
    id: Option<Value>,
}

/// Respuesta JSON-RPC 2.0 saliente
#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

impl JsonRpcResponse {
    fn success(id: Option<Value>, result: Value) -> Self {
        Self { jsonrpc: "2.0".into(), result: Some(result), error: None, id }
    }

    fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(JsonRpcError { code, message: message.into(), data: None }),
            id,
        }
    }

    fn invalid_request(message: impl Into<String>) -> Self {
        Self::error(None, -32600, message)
    }
}

// ---------------------------------------------------------------------------
// Estado del servidor
// ---------------------------------------------------------------------------

/// Estado compartido que reciben los handlers
struct McpState {
    db_path: String,
    ollama_url: String,
    embed_model: String,
    num_threads: usize,
}

// ---------------------------------------------------------------------------
// Definiciones de herramientas y recursos (MCP introspection)
// ---------------------------------------------------------------------------

/// Genera la lista de herramientas que expone el servidor
fn tool_definitions() -> Value {
    json!([
        {
            "name": "build",
            "description": "Construye o actualiza el grafo de conocimiento desde un directorio de notas Markdown",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "repo": { "type": "string", "description": "Ruta al directorio con archivos .md" },
                    "ner_model": { "type": "string", "description": "Modelo Ollama para extraer entidades", "default": "llama3.2:3b" },
                    "ollama_url": { "type": "string", "description": "URL de Ollama", "default": "http://localhost:11434" },
                    "embed_model": { "type": "string", "description": "Modelo de embeddings", "default": "nomic-embed-text" }
                },
                "required": ["repo"]
            }
        },
        {
            "name": "search",
            "description": "Búsqueda híbrida (vectores semánticos + expansión por grafo)",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Consulta de búsqueda" },
                    "k": { "type": "number", "description": "Número de resultados", "default": 5 },
                    "depth": { "type": "number", "description": "Profundidad de expansión en el grafo", "default": 2 },
                    "alpha": { "type": "number", "description": "Peso vectorial (0.0-1.0)", "default": 0.7 },
                    "vector_only": { "type": "boolean", "description": "Solo vectorial sin grafo", "default": false }
                },
                "required": ["query"]
            }
        },
        {
            "name": "fts",
            "description": "Búsqueda textual exacta (FTS5)",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Texto a buscar" },
                    "limit": { "type": "number", "description": "Máximo de resultados", "default": 10 }
                },
                "required": ["query"]
            }
        },
        {
            "name": "graph",
            "description": "Muestra los vecinos de un nodo en el grafo",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "label": { "type": "string", "description": "Etiqueta del nodo de partida" },
                    "depth": { "type": "number", "description": "Profundidad de expansión", "default": 2 }
                },
                "required": ["label"]
            }
        },
        {
            "name": "path",
            "description": "Encuentra el camino más corto entre dos nodos",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "Nodo origen" },
                    "to": { "type": "string", "description": "Nodo destino" },
                    "max_depth": { "type": "number", "description": "Profundidad máxima", "default": 10 }
                },
                "required": ["from", "to"]
            }
        },
        {
            "name": "stats",
            "description": "Estadísticas del grafo de conocimiento",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "seed",
            "description": "Puebla la base de datos con datos de demostración",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }
    ])
}

/// Genera la lista de recursos que expone el servidor
fn resource_definitions() -> Value {
    json!([
        {
            "uri": "graphrag://stats",
            "name": "Estadísticas del grafo",
            "description": "Resumen del grafo: nodos, aristas, tipos",
            "mimeType": "application/json"
        },
        {
            "uri": "graphrag://nodes",
            "name": "Todos los nodos",
            "description": "Lista completa de nodos del grafo",
            "mimeType": "application/json"
        },
        {
            "uri": "graphrag://edges",
            "name": "Todas las aristas",
            "description": "Lista completa de relaciones del grafo",
            "mimeType": "application/json"
        },
        {
            "uri": "graphrag://notes",
            "name": "Notas del grafo",
            "description": "Nodos de tipo 'note' con su contenido",
            "mimeType": "application/json"
        },
        {
            "uri": "graphrag://entities",
            "name": "Entidades extraídas",
            "description": "Nodos de tipo 'entity' o similares",
            "mimeType": "application/json"
        }
    ])
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

fn handle_initialize(req: &JsonRpcRequest) -> JsonRpcResponse {
    let client_version = req.params
        .as_ref()
        .and_then(|p| p.get("protocolVersion"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    info!("Cliente MCP conectado (protocolo: {})", client_version);

    JsonRpcResponse::success(req.id.clone(), json!({
        "protocolVersion": MCP_PROTOCOL_VERSION,
        "capabilities": {
            "tools": {},
            "resources": {}
        },
        "serverInfo": {
            "name": "graphrag",
            "version": env!("CARGO_PKG_VERSION")
        }
    }))
}

fn handle_tools_list(req: &JsonRpcRequest) -> JsonRpcResponse {
    JsonRpcResponse::success(req.id.clone(), json!({
        "tools": tool_definitions()
    }))
}

fn handle_resources_list(req: &JsonRpcRequest) -> JsonRpcResponse {
    JsonRpcResponse::success(req.id.clone(), json!({
        "resources": resource_definitions()
    }))
}

fn handle_resources_read(req: &JsonRpcRequest, state: &McpState) -> JsonRpcResponse {
    let uri = match req.params
        .as_ref()
        .and_then(|p| p.get("uri"))
        .and_then(|u| u.as_str())
    {
        Some(u) => u,
        None => return JsonRpcResponse::error(req.id.clone(), -32602, "Missing 'uri' parameter"),
    };

    let result = match uri {
        "graphrag://stats" => handle_resource_stats(&state.db_path),
        "graphrag://nodes" => handle_resource_nodes(&state.db_path, None),
        "graphrag://notes" => handle_resource_nodes(&state.db_path, Some("note")),
        "graphrag://entities" => handle_resource_nodes(&state.db_path, Some("entity")),
        "graphrag://edges" => handle_resource_edges(&state.db_path, None),
        u if u.starts_with("graphrag://nodes/") => {
            let id = u.trim_start_matches("graphrag://nodes/");
            handle_resource_node_by_label(&state.db_path, id)
        }
        u => {
            return JsonRpcResponse::error(req.id.clone(), -32602, format!("Unknown resource URI: {}", u));
        }
    };

    match result {
        Ok(text) => JsonRpcResponse::success(req.id.clone(), json!({
            "contents": [{
                "uri": uri,
                "mimeType": "application/json",
                "text": text
            }]
        })),
        Err(e) => JsonRpcResponse::error(req.id.clone(), -32603, format!("Error reading resource: {}", e)),
    }
}

fn handle_tools_call(req: &JsonRpcRequest, state: &McpState) -> JsonRpcResponse {
    let name = match req.params.as_ref().and_then(|p| p.get("name")).and_then(|n| n.as_str()) {
        Some(n) => n,
        None => return JsonRpcResponse::error(req.id.clone(), -32602, "Missing tool 'name'"),
    };

    let default_args = json!({});
    let args = req.params.as_ref().and_then(|p| p.get("arguments")).unwrap_or(&default_args);

    info!("Tool call: {} args={}", name, args);

    let result = match name {
        "build" => handle_tool_build(args, state),
        "search" => handle_tool_search(args, state),
        "fts" => handle_tool_fts(args, state),
        "graph" => handle_tool_graph(args, state),
        "path" => handle_tool_path(args, state),
        "stats" => handle_tool_stats(args, state),
        "seed" => handle_tool_seed(args, state),
        other => return JsonRpcResponse::error(req.id.clone(), -32601, format!("Unknown tool: {}", other)),
    };

    match result {
        Ok(text) => JsonRpcResponse::success(req.id.clone(), json!({
            "content": [{"type": "text", "text": text}]
        })),
        Err(e) => JsonRpcResponse::error(req.id.clone(), -32603, format!("Error: {}", e)),
    }
}

// ---------------------------------------------------------------------------
// Handlers de herramientas
// ---------------------------------------------------------------------------

fn handle_tool_build(args: &Value, state: &McpState) -> Result<String> {
    let repo = args.get("repo")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'repo' argument"))?;

    let ner_model = args.get("ner_model").and_then(|v| v.as_str()).unwrap_or("llama3.2:3b");
    let ollama_url = args.get("ollama_url").and_then(|v| v.as_str()).unwrap_or(&state.ollama_url);
    let embed_model = args.get("embed_model").and_then(|v| v.as_str()).unwrap_or(&state.embed_model);

    let stats = graph::build::build_graph(repo, &state.db_path, ollama_url, ner_model, embed_model, state.num_threads)?;
    Ok(format!("✅ Grafo construido:\n{}", stats))
}

fn handle_tool_search(args: &Value, state: &McpState) -> Result<String> {
    let query = args.get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'query' argument"))?;

    let k = args.get("k").and_then(|v| v.as_f64()).unwrap_or(5.0) as usize;
    let depth = args.get("depth").and_then(|v| v.as_f64()).unwrap_or(2.0) as i32;
    let alpha = args.get("alpha").and_then(|v| v.as_f64()).unwrap_or(0.7);
    let vector_only = args.get("vector_only").and_then(|v| v.as_bool()).unwrap_or(false);

    let mut hs = HybridSearch::new(&state.db_path, None)?;

    let results = if vector_only {
        hs.vector_only(query, k)?
    } else {
        hs.hybrid_search(query, k, depth, alpha, None)?
    };

    if results.is_empty() {
        return Ok("(sin resultados)".to_string());
    }

    let mut out = format!("🔍 Resultados para '{}':\n\n", query);
    for (i, r) in results.iter().enumerate() {
        let file_str = if !r.file.is_empty() {
            format!("  ({})", r.file)
        } else {
            String::new()
        };
        out.push_str(&format!("{}. [{:.3}] {}{} ({})\n", i + 1, r.score, r.label, file_str, r.r#type));
        if !r.content.is_empty() {
            let snippet = if r.content.len() > 120 {
                format!("{}...", &r.content[..120])
            } else {
                r.content.clone()
            };
            out.push_str(&format!("   {}\n", snippet));
        }
        if !r.neighbors.is_empty() {
            let n_list: Vec<String> = r.neighbors.iter()
                .map(|n| format!("{} (dist {})", n.label, n.distance))
                .collect();
            out.push_str(&format!("   Vecinos: {}\n", n_list.join(", ")));
        }
    }
    Ok(out)
}

fn handle_tool_fts(args: &Value, state: &McpState) -> Result<String> {
    let query = args.get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'query' argument"))?;

    let limit = args.get("limit").and_then(|v| v.as_f64()).unwrap_or(10.0) as usize;

    let hs = HybridSearch::new(&state.db_path, None)?;
    let results = hs.fts_search(query, limit)?;

    if results.is_empty() {
        return Ok("(sin resultados)".to_string());
    }

    let mut out = format!("📄 Resultados FTS para '{}':\n\n", query);
    for (i, r) in results.iter().enumerate() {
        out.push_str(&format!("{}. [{:.3}] {}\n", i + 1, r.score, r.label));
    }
    Ok(out)
}

fn handle_tool_graph(args: &Value, state: &McpState) -> Result<String> {
    let label = args.get("label")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'label' argument"))?;

    let depth = args.get("depth").and_then(|v| v.as_f64()).unwrap_or(2.0) as i32;

    let hs = HybridSearch::new(&state.db_path, None)?;
    let neighbors = hs.graph_only(label, depth)?;

    if neighbors.is_empty() {
        return Ok(format!("🔗 '{}' no tiene vecinos (depth={})", label, depth));
    }

    let mut out = format!("🔗 Vecinos de '{}' (depth={}):\n\n", label, depth);
    for (i, n) in neighbors.iter().enumerate() {
        out.push_str(&format!("{}. [dist {}] {} ({})\n", i + 1, n.distance, n.label, n.r#type));
    }
    Ok(out)
}

fn handle_tool_path(args: &Value, state: &McpState) -> Result<String> {
    let from = args.get("from")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'from' argument"))?;

    let to = args.get("to")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'to' argument"))?;

    let max_depth = args.get("max_depth").and_then(|v| v.as_f64()).unwrap_or(10.0) as i32;

    let conn = rusqlite::Connection::open(&state.db_path)?;
    let path = graph::expand::shortest_path(&conn, from, to, max_depth)?;

    match path {
        Some(nodes) => Ok(format!("🛤️  '{}' → '{}': {}", from, to, nodes.join(" → "))),
        None => Ok(format!("⚠️  No se encontró camino entre '{}' y '{}'", from, to)),
    }
}

fn handle_tool_stats(_args: &Value, state: &McpState) -> Result<String> {
    let hs = HybridSearch::new(&state.db_path, None)?;
    let stats = hs.stats()?;
    Ok(stats.to_string())
}

fn handle_tool_seed(_args: &Value, state: &McpState) -> Result<String> {
    crate::seed::demo_data::create_demo_db(&state.db_path)?;
    Ok(format!("✅ Base de datos demo creada en {}", state.db_path))
}

// ---------------------------------------------------------------------------
// Handlers de recursos
// ---------------------------------------------------------------------------

fn handle_resource_stats(db_path: &str) -> Result<String> {
    let hs = HybridSearch::new(db_path, None)?;
    let stats = hs.stats()?;
    Ok(json!({
        "nodes": stats.nodes,
        "edges": stats.edges,
        "types": format!("{}", stats)
    }).to_string())
}

fn handle_resource_nodes(db_path: &str, filter_type: Option<&str>) -> Result<String> {
    let conn = rusqlite::Connection::open(db_path)?;

    let (sql, label) = match filter_type {
        Some(t) => (format!("SELECT id, label, type, metadata FROM nodes WHERE type = ?1"), t.to_string()),
        None => ("SELECT id, label, type, metadata FROM nodes".to_string(), "all".to_string()),
    };

    let mut stmt = conn.prepare(&sql)?;
    let rows: Vec<Value> = if filter_type.is_some() {
        stmt.query_map([&label], |row| {
            let id: i64 = row.get(0)?;
            let label: String = row.get(1)?;
            let type_: String = row.get(2)?;
            let meta: String = row.get::<_, Option<String>>(3)?.unwrap_or_default();
            Ok(json!({"id": id, "label": label, "type": type_, "metadata": meta}))
        })?.filter_map(|r| r.ok()).collect()
    } else {
        stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let label: String = row.get(1)?;
            let type_: String = row.get(2)?;
            let meta: String = row.get::<_, Option<String>>(3)?.unwrap_or_default();
            Ok(json!({"id": id, "label": label, "type": type_, "metadata": meta}))
        })?.filter_map(|r| r.ok()).collect()
    };

    Ok(json!({"nodes": rows, "total": rows.len()}).to_string())
}

fn handle_resource_node_by_label(db_path: &str, label: &str) -> Result<String> {
    let conn = rusqlite::Connection::open(db_path)?;
    let mut stmt = conn.prepare("SELECT id, label, type, metadata FROM nodes WHERE label = ?1 OR id = ?2")?;

    let id_parse = label.parse::<i64>().unwrap_or(-1);

    let node: Option<Value> = stmt.query_row(
        rusqlite::params![label, id_parse],
        |row| {
            let id: i64 = row.get(0)?;
            let label: String = row.get(1)?;
            let type_: String = row.get(2)?;
            let meta: String = row.get::<_, Option<String>>(3)?.unwrap_or_default();
            Ok(json!({"id": id, "label": label, "type": type_, "metadata": meta}))
        }
    ).ok();

    match node {
        Some(n) => Ok(json!({"node": n}).to_string()),
        None => Ok(json!({"error": format!("Node '{}' not found", label)}).to_string()),
    }
}

fn handle_resource_edges(db_path: &str, _type_filter: Option<&str>) -> Result<String> {
    let conn = rusqlite::Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT e.id, e.source_id, e.target_id, e.type, e.weight, e.context,
                s.label AS source_label, t.label AS target_label
         FROM edges e
         JOIN nodes s ON e.source_id = s.id
         JOIN nodes t ON e.target_id = t.id
         LIMIT 500"
    )?;

    let rows: Vec<Value> = stmt.query_map([], |row| {
        let id: i64 = row.get(0)?;
        let _src_id: i64 = row.get(1)?;
        let _tgt_id: i64 = row.get(2)?;
        let type_: String = row.get(3)?;
        let weight: f64 = row.get(4)?;
        let ctx: String = row.get::<_, Option<String>>(5)?.unwrap_or_default();
        let src_lbl: String = row.get(6)?;
        let tgt_lbl: String = row.get(7)?;
        Ok(json!({"id": id, "source": src_lbl, "target": tgt_lbl,
                   "type": type_, "weight": weight, "context": ctx}))
    })?.filter_map(|r| r.ok()).collect();

    Ok(json!({"edges": rows, "total": rows.len()}).to_string())
}

// ---------------------------------------------------------------------------
// Dispatcher principal
// ---------------------------------------------------------------------------

/// Procesa una petición JSON-RPC y devuelve la respuesta
fn dispatch(req: JsonRpcRequest, state: &McpState) -> JsonRpcResponse {
    match req.method.as_str() {
        "initialize" => handle_initialize(&req),
        "notifications/initialized" | "notifications/cancelled" => {
            // Notificaciones sin respuesta
            JsonRpcResponse { jsonrpc: "2.0".into(), result: None, error: None, id: None }
        }
        "shutdown" => {
            // Señal de apagado — el bucle principal lo maneja
            JsonRpcResponse::success(req.id.clone(), json!(null))
        }
        "tools/list" => handle_tools_list(&req),
        "tools/call" => handle_tools_call(&req, state),
        "resources/list" => handle_resources_list(&req),
        "resources/read" => handle_resources_read(&req, state),
        _ => JsonRpcResponse::error(req.id, -32601, format!("Method not found: {}", req.method)),
    }
}

// ---------------------------------------------------------------------------
// Bucle principal del servidor
// ---------------------------------------------------------------------------

/// Inicia el servidor MCP sobre stdio.
///
/// Lee líneas JSON-RPC de stdin, las procesa y escribe la respuesta en stdout.
/// El servidor se ejecuta hasta recibir EOF en stdin.
pub fn run_server(db_path: &str, ollama_url: &str, embed_model: &str, num_threads: usize) -> Result<()> {
    let state = McpState {
        db_path: db_path.to_string(),
        ollama_url: ollama_url.to_string(),
        embed_model: embed_model.to_string(),
        num_threads,
    };

    info!("🧠 Servidor MCP GraphRAG iniciado (db: {})", db_path);

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                error!("Error leyendo stdin: {}", e);
                break;
            }
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Parsear la petición
        let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse::invalid_request(format!("Invalid JSON: {}", e));
                let json = serde_json::to_string(&resp)?;
                writeln!(stdout_lock, "{}", json)?;
                stdout_lock.flush()?;
                continue;
            }
        };

        // Detectar shutdown
        if req.method == "shutdown" {
            let resp = dispatch(req, &state);
            if let Some(_id) = &resp.id {
                let json = serde_json::to_string(&resp)?;
                writeln!(stdout_lock, "{}", json)?;
                stdout_lock.flush()?;
            }
            break;
        }

        // Notificaciones (sin id) no tienen respuesta
        if req.id.is_none() {
            dispatch(req, &state);
            continue;
        }

        // Procesar y responder
        let resp = dispatch(req, &state);
        let json = serde_json::to_string(&resp)?;
        writeln!(stdout_lock, "{}", json)?;
        stdout_lock.flush()?;
    }

    info!("Servidor MCP finalizado");
    Ok(())
}