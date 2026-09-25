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

use anyhow::Result;
use log::{error, info};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

use crate::community;
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
        Self {
            jsonrpc: "2.0".into(),
            result: Some(result),
            error: None,
            id,
        }
    }

    fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
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
                    "depth": { "type": "number", "description": "Profundidad de expansión en el grafo (0 = solo vectorial)", "default": 2 },
                    "alpha": { "type": "number", "description": "Peso vectorial (0.0-1.0)", "default": 0.7 },
                    "filter": { "type": "array", "items": { "type": "string" }, "description": "Filtros de metadatos (repeatable): 'campo operador valor'" }
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
        },
        {
            "name": "community_detect",
            "description": "Detecta comunidades en el grafo de entidades usando el algoritmo Leiden",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "resolution": { "type": "number", "description": "Parámetro de resolución para CPM (default: 1.0)", "default": 1.0 }
                }
            }
        },
        {
            "name": "community_summarize",
            "description": "Genera resúmenes LLM para todas las comunidades detectadas",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "summary_model": { "type": "string", "description": "Modelo para generación de resúmenes", "default": "llama3.2:3b" },
                    "ollama_url": { "type": "string", "description": "URL de Ollama", "default": "http://localhost:11434" },
                    "embed_model": { "type": "string", "description": "Modelo de embeddings", "default": "nomic-embed-text" }
                }
            }
        },
        {
            "name": "search_answer",
            "description": "Búsqueda con respuesta narrativa usando contexto de comunidades + chunks",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Consulta" },
                    "k": { "type": "number", "description": "Número de resultados", "default": 5 },
                    "depth": { "type": "number", "description": "Profundidad de expansión (0 = solo vectorial)", "default": 2 },
                    "alpha": { "type": "number", "description": "Peso vectorial (0.0-1.0)", "default": 0.7 },
                    "summary_model": { "type": "string", "description": "Modelo para generación de respuesta", "default": "llama3.2:3b" }
                },
                "required": ["query"]
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
        },
        {
            "uri": "graphrag://communities",
            "name": "Todas las comunidades",
            "description": "Lista de comunidades detectadas con sus resúmenes",
            "mimeType": "application/json"
        }
    ])
}

fn resource_template_definitions() -> Value {
    json!([
        {
            "uriTemplate": "graphrag://communities/{id}",
            "name": "Comunidad por ID",
            "description": "Detalle de una comunidad específica (ID numérico)",
            "mimeType": "application/json"
        }
    ])
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

fn handle_initialize(req: &JsonRpcRequest) -> JsonRpcResponse {
    let client_version = req
        .params
        .as_ref()
        .and_then(|p| p.get("protocolVersion"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    info!("Cliente MCP conectado (protocolo: {})", client_version);

    JsonRpcResponse::success(
        req.id.clone(),
        json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {
                "tools": {},
                "resources": {}
            },
            "serverInfo": {
                "name": "graphrag",
                "version": env!("CARGO_PKG_VERSION")
            }
        }),
    )
}

fn handle_tools_list(req: &JsonRpcRequest) -> JsonRpcResponse {
    JsonRpcResponse::success(
        req.id.clone(),
        json!({
            "tools": tool_definitions()
        }),
    )
}

fn handle_resources_list(req: &JsonRpcRequest) -> JsonRpcResponse {
    JsonRpcResponse::success(
        req.id.clone(),
        json!({
            "resources": resource_definitions(),
            "resourceTemplates": resource_template_definitions()
        }),
    )
}

fn handle_resources_read(req: &JsonRpcRequest, state: &McpState) -> JsonRpcResponse {
    let uri = match req
        .params
        .as_ref()
        .and_then(|p| p.get("uri"))
        .and_then(|u| u.as_str())
    {
        Some(u) => u,
        None => return JsonRpcResponse::error(req.id.clone(), -32602, "Missing 'uri' parameter"),
    };

    let result = match uri {
        "graphrag://stats" => {
            handle_resource_stats(&state.db_path, &state.ollama_url, &state.embed_model)
        }
        "graphrag://nodes" => handle_resource_nodes(&state.db_path, None),
        "graphrag://notes" => handle_resource_nodes(&state.db_path, Some("note")),
        "graphrag://entities" => handle_resource_nodes(&state.db_path, Some("entity")),
        "graphrag://communities" => handle_resource_communities(&state.db_path),
        "graphrag://edges" => handle_resource_edges(&state.db_path, None),
        u if u.starts_with("graphrag://nodes/") => {
            let id = u.trim_start_matches("graphrag://nodes/");
            handle_resource_node_by_label(&state.db_path, id)
        }
        u if u.starts_with("graphrag://communities/") => {
            let id = u.trim_start_matches("graphrag://communities/");
            handle_resource_community_by_id(&state.db_path, id)
        }
        u => {
            return JsonRpcResponse::error(
                req.id.clone(),
                -32602,
                format!("Unknown resource URI: {}", u),
            );
        }
    };

    match result {
        Ok(text) => JsonRpcResponse::success(
            req.id.clone(),
            json!({
                "contents": [{
                    "uri": uri,
                    "mimeType": "application/json",
                    "text": text
                }]
            }),
        ),
        Err(e) => JsonRpcResponse::error(
            req.id.clone(),
            -32603,
            format!("Error reading resource: {}", e),
        ),
    }
}

fn handle_tools_call(req: &JsonRpcRequest, state: &McpState) -> JsonRpcResponse {
    let name = match req
        .params
        .as_ref()
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
    {
        Some(n) => n,
        None => return JsonRpcResponse::error(req.id.clone(), -32602, "Missing tool 'name'"),
    };

    let default_args = json!({});
    let args = req
        .params
        .as_ref()
        .and_then(|p| p.get("arguments"))
        .unwrap_or(&default_args);

    info!("Tool call: {} args={}", name, args);

    let result = match name {
        "build" => handle_tool_build(args, state),
        "search" => handle_tool_search(args, state),
        "fts" => handle_tool_fts(args, state),
        "graph" => handle_tool_graph(args, state),
        "path" => handle_tool_path(args, state),
        "stats" => handle_tool_stats(args, state),
        "seed" => handle_tool_seed(args, state),
        "community_detect" => handle_tool_community_detect(args, state),
        "community_summarize" => handle_tool_community_summarize(args, state),
        "search_answer" => handle_tool_search_answer(args, state),
        other => {
            return JsonRpcResponse::error(
                req.id.clone(),
                -32601,
                format!("Unknown tool: {}", other),
            )
        }
    };

    match result {
        Ok(text) => JsonRpcResponse::success(
            req.id.clone(),
            json!({
                "content": [{"type": "text", "text": text}]
            }),
        ),
        Err(e) => JsonRpcResponse::error(req.id.clone(), -32603, format!("Error: {}", e)),
    }
}

// ---------------------------------------------------------------------------
// Handlers de herramientas
// ---------------------------------------------------------------------------

fn handle_tool_build(args: &Value, state: &McpState) -> Result<String> {
    let repo = args
        .get("repo")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'repo' argument"))?;

    let ner_model = args
        .get("ner_model")
        .and_then(|v| v.as_str())
        .unwrap_or("llama3.2:3b");
    let ollama_url = args
        .get("ollama_url")
        .and_then(|v| v.as_str())
        .unwrap_or(&state.ollama_url);
    let embed_model = args
        .get("embed_model")
        .and_then(|v| v.as_str())
        .unwrap_or(&state.embed_model);

    let stats = graph::build::build_graph(
        repo,
        &state.db_path,
        ollama_url,
        ner_model,
        embed_model,
        state.num_threads,
    )?;
    Ok(format!("✅ Grafo construido:\n{}", stats))
}

fn handle_tool_search(args: &Value, state: &McpState) -> Result<String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'query' argument"))?;

    let k = args.get("k").and_then(|v| v.as_f64()).unwrap_or(5.0) as usize;
    let depth = args.get("depth").and_then(|v| v.as_f64()).unwrap_or(2.0) as i32;
    let alpha = args.get("alpha").and_then(|v| v.as_f64()).unwrap_or(0.7);

    // Parse optional filters from MCP args (array of strings)
    let filters: Vec<crate::search::filter::Filter> = args
        .get("filter")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(crate::search::filter::parse_filter)
                .collect::<anyhow::Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();

    let ollama = crate::embed::ollama::OllamaClient::new(&state.ollama_url, &state.embed_model);
    let mut hs = HybridSearch::new(&state.db_path, ollama)?;

    let results = hs.hybrid_search(query, k, depth, alpha, None, false, &filters)?;

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
        out.push_str(&format!(
            "{}. [{:.3}] {}{} ({})\n",
            i + 1,
            r.score,
            r.label,
            file_str,
            r.r#type
        ));
        if !r.chunk_header.is_empty() {
            out.push_str(&format!("   ├── {}\n", r.chunk_header));
        }
        if !r.content.is_empty() {
            out.push_str(&format!("   └── {}\n", r.content));
        }
        if !r.neighbors.is_empty() {
            let n_list: Vec<String> = r
                .neighbors
                .iter()
                .map(|n| format!("{} (dist {})", n.label, n.distance))
                .collect();
            out.push_str(&format!("   Vecinos: {}\n", n_list.join(", ")));
        }
    }
    Ok(out)
}

fn handle_tool_fts(args: &Value, state: &McpState) -> Result<String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'query' argument"))?;

    let limit = args.get("limit").and_then(|v| v.as_f64()).unwrap_or(10.0) as usize;

    let ollama = crate::embed::ollama::OllamaClient::new(&state.ollama_url, &state.embed_model);
    let hs = HybridSearch::new(&state.db_path, ollama)?;
    let results = hs.fts_search(query, limit, false)?;

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
    let label = args
        .get("label")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'label' argument"))?;

    let depth = args.get("depth").and_then(|v| v.as_f64()).unwrap_or(2.0) as i32;

    let ollama = crate::embed::ollama::OllamaClient::new(&state.ollama_url, &state.embed_model);
    let hs = HybridSearch::new(&state.db_path, ollama)?;
    let neighbors = hs.graph_only(label, depth)?;

    if neighbors.is_empty() {
        return Ok(format!("🔗 '{}' no tiene vecinos (depth={})", label, depth));
    }

    let mut out = format!("🔗 Vecinos de '{}' (depth={}):\n\n", label, depth);
    for (i, n) in neighbors.iter().enumerate() {
        out.push_str(&format!(
            "{}. [dist {}] {} ({})\n",
            i + 1,
            n.distance,
            n.label,
            n.r#type
        ));
    }
    Ok(out)
}

fn handle_tool_path(args: &Value, state: &McpState) -> Result<String> {
    let from = args
        .get("from")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'from' argument"))?;

    let to = args
        .get("to")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'to' argument"))?;

    let max_depth = args
        .get("max_depth")
        .and_then(|v| v.as_f64())
        .unwrap_or(10.0) as i32;

    let conn = rusqlite::Connection::open(&state.db_path)?;
    let path = graph::expand::shortest_path(&conn, from, to, max_depth)?;

    match path {
        Some(nodes) => Ok(format!("🛤️  '{}' → '{}': {}", from, to, nodes.join(" → "))),
        None => Ok(format!(
            "⚠️  No se encontró camino entre '{}' y '{}'",
            from, to
        )),
    }
}

fn handle_tool_stats(_args: &Value, state: &McpState) -> Result<String> {
    let ollama = crate::embed::ollama::OllamaClient::new(&state.ollama_url, &state.embed_model);
    let hs = HybridSearch::new(&state.db_path, ollama)?;
    let stats = hs.stats()?;
    Ok(stats.to_string())
}

fn handle_tool_seed(_args: &Value, state: &McpState) -> Result<String> {
    crate::seed::demo_data::create_demo_db(&state.db_path, &state.ollama_url, &state.embed_model)?;
    Ok(format!("✅ Base de datos demo creada en {}", state.db_path))
}

fn handle_tool_community_detect(args: &Value, state: &McpState) -> Result<String> {
    let resolution = args
        .get("resolution")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);
    let (level1, level2) = community::detect::run_community_detection(&state.db_path, resolution)?;
    Ok(format!(
        "✅ Comunidades detectadas: {} nivel 1, {} nivel 2",
        level1, level2
    ))
}

fn handle_tool_community_summarize(args: &Value, state: &McpState) -> Result<String> {
    let ollama_url = args
        .get("ollama_url")
        .and_then(|v| v.as_str())
        .unwrap_or(&state.ollama_url);
    let summary_model = args
        .get("summary_model")
        .and_then(|v| v.as_str())
        .unwrap_or("llama3.2:3b");
    let embed_model = args
        .get("embed_model")
        .and_then(|v| v.as_str())
        .unwrap_or(&state.embed_model);
    let (count, tokens) = community::summarize::summarize_all_communities(
        &state.db_path,
        ollama_url,
        summary_model,
        embed_model,
    )?;
    if count == 0 {
        Ok("✅ Todas las comunidades ya tienen resumen".to_string())
    } else {
        Ok(format!(
            "✅ {} comunidades resumidas ({} tokens)",
            count, tokens
        ))
    }
}

fn handle_tool_search_answer(args: &Value, state: &McpState) -> Result<String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'query' argument"))?;
    let k = args.get("k").and_then(|v| v.as_f64()).unwrap_or(5.0) as usize;
    let depth = args.get("depth").and_then(|v| v.as_f64()).unwrap_or(2.0) as i32;
    let alpha = args.get("alpha").and_then(|v| v.as_f64()).unwrap_or(0.7);
    let summary_model = args
        .get("summary_model")
        .and_then(|v| v.as_str())
        .unwrap_or("llama3.2:3b");

    let ollama = crate::embed::ollama::OllamaClient::new(&state.ollama_url, &state.embed_model);
    let mut hs = HybridSearch::new(&state.db_path, ollama.clone())?;
    let results = hs.hybrid_search(query, k, depth, alpha, None, false, &[])?;

    let conn = rusqlite::Connection::open(&state.db_path)?;
    let community_embeddings =
        crate::db::communities::load_all_community_embeddings(&conn).unwrap_or_default();

    let answer = community::search::answer_query(
        &ollama,
        query,
        &results,
        &community_embeddings,
        summary_model,
    )?;
    Ok(answer)
}

// ---------------------------------------------------------------------------
// Handlers de recursos
// ---------------------------------------------------------------------------

fn handle_resource_stats(db_path: &str, ollama_url: &str, embed_model: &str) -> Result<String> {
    let ollama = crate::embed::ollama::OllamaClient::new(ollama_url, embed_model);
    let hs = HybridSearch::new(db_path, ollama)?;
    let stats = hs.stats()?;
    Ok(json!({
        "nodes": stats.nodes,
        "edges": stats.edges,
        "types": format!("{}", stats)
    })
    .to_string())
}

fn handle_resource_nodes(db_path: &str, filter_type: Option<&str>) -> Result<String> {
    let conn = rusqlite::Connection::open(db_path)?;

    let (sql, label) = match filter_type {
        Some(t) => (
            "SELECT id, label, type, metadata FROM nodes WHERE type = ?1".to_string(),
            t.to_string(),
        ),
        None => (
            "SELECT id, label, type, metadata FROM nodes".to_string(),
            "all".to_string(),
        ),
    };

    let mut stmt = conn.prepare(&sql)?;
    let rows: Vec<Value> = if filter_type.is_some() {
        stmt.query_map([&label], |row| {
            let id: i64 = row.get(0)?;
            let label: String = row.get(1)?;
            let type_: String = row.get(2)?;
            let meta: String = row.get::<_, Option<String>>(3)?.unwrap_or_default();
            Ok(json!({"id": id, "label": label, "type": type_, "metadata": meta}))
        })?
        .filter_map(|r| r.ok())
        .collect()
    } else {
        stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let label: String = row.get(1)?;
            let type_: String = row.get(2)?;
            let meta: String = row.get::<_, Option<String>>(3)?.unwrap_or_default();
            Ok(json!({"id": id, "label": label, "type": type_, "metadata": meta}))
        })?
        .filter_map(|r| r.ok())
        .collect()
    };

    Ok(json!({"nodes": rows, "total": rows.len()}).to_string())
}

fn handle_resource_node_by_label(db_path: &str, label: &str) -> Result<String> {
    let conn = rusqlite::Connection::open(db_path)?;
    let mut stmt =
        conn.prepare("SELECT id, label, type, metadata FROM nodes WHERE label = ?1 OR id = ?2")?;

    let id_parse = label.parse::<i64>().unwrap_or(-1);

    let node: Option<Value> = stmt
        .query_row(rusqlite::params![label, id_parse], |row| {
            let id: i64 = row.get(0)?;
            let label: String = row.get(1)?;
            let type_: String = row.get(2)?;
            let meta: String = row.get::<_, Option<String>>(3)?.unwrap_or_default();
            Ok(json!({"id": id, "label": label, "type": type_, "metadata": meta}))
        })
        .ok();

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
         LIMIT 500",
    )?;

    let rows: Vec<Value> = stmt
        .query_map([], |row| {
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
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(json!({"edges": rows, "total": rows.len()}).to_string())
}

fn handle_resource_communities(db_path: &str) -> Result<String> {
    let conn = rusqlite::Connection::open(db_path)?;
    let communities = crate::db::communities::get_all_communities(&conn)?;
    let json = serde_json::to_string_pretty(
        &communities
            .iter()
            .map(|c| {
                serde_json::json!({
                    "id": c.id,
                    "label": c.label,
                    "level": c.level,
                    "parent_id": c.parent_id,
                    "summary": c.summary,
                    "member_count": c.member_count,
                    "algorithm": c.algorithm,
                    "summary_tokens": c.summary_tokens,
                })
            })
            .collect::<Vec<_>>(),
    )?;
    Ok(json)
}

fn handle_resource_community_by_id(db_path: &str, id_str: &str) -> Result<String> {
    let id: i64 = id_str
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid community ID: {}", id_str))?;
    let conn = rusqlite::Connection::open(db_path)?;
    let community = crate::db::communities::get_community_by_id(&conn, id)?
        .ok_or_else(|| anyhow::anyhow!("Community not found: {}", id))?;
    let json = serde_json::to_string_pretty(&serde_json::json!({
        "id": community.id,
        "label": community.label,
        "level": community.level,
        "parent_id": community.parent_id,
        "summary": community.summary,
        "member_ids": community.member_ids,
        "member_count": community.member_count,
        "algorithm": community.algorithm,
        "quality_fn": community.quality_fn,
        "resolution": community.resolution,
        "summary_model": community.summary_model,
        "summary_tokens": community.summary_tokens,
    }))?;
    Ok(json)
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
            JsonRpcResponse {
                jsonrpc: "2.0".into(),
                result: None,
                error: None,
                id: None,
            }
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
pub fn run_server(
    db_path: &str,
    ollama_url: &str,
    embed_model: &str,
    num_threads: usize,
) -> Result<()> {
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
