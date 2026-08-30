use anyhow::Result;
use log::debug;
use rusqlite::Connection;
use serde::Serialize;

/// Un vecino encontrado durante la expansión del grafo
#[derive(Debug, Clone, Serialize)]
pub struct Neighbor {
    pub id: i64,
    pub label: String,
    pub r#type: String,
    pub distance: i32,
}

/// Expande vecinos desde un nodo usando CTE recursiva en ambas direcciones.
///
/// La CTE empieza con los vecinos directos del nodo (nivel 0)
/// y va expandiendo nivel a nivel hasta la profundidad indicada.
/// Recorre aristas en ambas direcciones (source → target y target → source).
pub fn expand_neighbors(conn: &Connection, node_id: i64, depth: i32) -> Result<Vec<Neighbor>> {
    if depth <= 0 {
        return Ok(Vec::new());
    }

    let mut stmt = conn.prepare_cached(
        r#"
        WITH RECURSIVE neighbors AS (
            -- Vecinos directos (nivel 0)
            SELECT source_id AS id, 0 AS level
            FROM edges WHERE target_id = ?1
            UNION
            SELECT target_id, 0
            FROM edges WHERE source_id = ?1
            UNION ALL
            -- Expansión recursiva
            SELECT e.target_id, n.level + 1
            FROM edges e
            JOIN neighbors n ON e.source_id = n.id
            WHERE n.level < ?2
            UNION ALL
            SELECT e.source_id, n.level + 1
            FROM edges e
            JOIN neighbors n ON e.target_id = n.id
            WHERE n.level < ?2
        )
        SELECT DISTINCT n.id, nodes.label, nodes.type, MIN(n.level) AS distance
        FROM neighbors n
        JOIN nodes ON nodes.id = n.id
        WHERE n.id != ?1
        GROUP BY n.id
        ORDER BY distance, nodes.type
        LIMIT 20
        "#,
    )?;

    let rows = stmt.query_map(rusqlite::params![node_id, depth], |row| {
        Ok(Neighbor {
            id: row.get(0)?,
            label: row.get(1)?,
            r#type: row.get(2)?,
            distance: row.get(3)?,
        })
    })?;

    let mut neighbors = Vec::new();
    for row in rows {
        neighbors.push(row?);
    }
    debug!(
        "expand_neighbors: node_id={}, depth={} → {} vecinos",
        node_id,
        depth,
        neighbors.len()
    );

    Ok(neighbors)
}

/// Expande vecinos filtrando por peso mínimo de arista.
/// Las aristas con weight bajo (co-ocurrencias accidentales)
/// no merece la pena expandirlas.
pub fn expand_neighbors_weighted(
    conn: &Connection,
    node_id: i64,
    depth: i32,
    min_weight: f64,
) -> Result<Vec<Neighbor>> {
    if depth <= 0 {
        return Ok(Vec::new());
    }

    let mut stmt = conn.prepare_cached(
        r#"
        WITH RECURSIVE neighbors AS (
            SELECT source_id AS id, 0 AS level
            FROM edges WHERE target_id = ?1 AND weight >= ?3
            UNION
            SELECT target_id, 0
            FROM edges WHERE source_id = ?1 AND weight >= ?3
            UNION ALL
            SELECT e.target_id, n.level + 1
            FROM edges e
            JOIN neighbors n ON e.source_id = n.id
            WHERE n.level < ?2 AND e.weight >= ?3
            UNION ALL
            SELECT e.source_id, n.level + 1
            FROM edges e
            JOIN neighbors n ON e.target_id = n.id
            WHERE n.level < ?2 AND e.weight >= ?3
        )
        SELECT DISTINCT n.id, nodes.label, nodes.type, MIN(n.level) AS distance
        FROM neighbors n
        JOIN nodes ON nodes.id = n.id
        WHERE n.id != ?1
        GROUP BY n.id
        ORDER BY distance, nodes.type
        LIMIT 20
        "#,
    )?;

    let rows = stmt.query_map(rusqlite::params![node_id, depth, min_weight], |row| {
        Ok(Neighbor {
            id: row.get(0)?,
            label: row.get(1)?,
            r#type: row.get(2)?,
            distance: row.get(3)?,
        })
    })?;

    let mut neighbors = Vec::new();
    for row in rows {
        neighbors.push(row?);
    }
    debug!(
        "expand_neighbors_weighted: node_id={}, depth={}, min_weight={} → {} vecinos",
        node_id,
        depth,
        min_weight,
        neighbors.len()
    );

    Ok(neighbors)
}

/// Encuentra el camino más corto entre dos nodos usando CTE recursiva
/// con protección contra ciclos mediante acumulador de visitados.
pub fn shortest_path(
    conn: &Connection,
    from_label: &str,
    to_label: &str,
    max_depth: i32,
) -> Result<Option<Vec<String>>> {
    let mut stmt = conn.prepare_cached(
        r#"
        WITH RECURSIVE path(from_id, to_id, route, depth, visited) AS (
            SELECT n.id, n.id, n.label, 0, ',' || CAST(n.id AS TEXT) || ','
            FROM nodes n
            WHERE n.label = ?1

            UNION ALL

            SELECT p.from_id,
                   CASE WHEN e.source_id = p.to_id THEN e.target_id ELSE e.source_id END,
                   p.route || ' --> ' || COALESCE(n2.label, '?'),
                   p.depth + 1,
                   p.visited || COALESCE(CAST(
                       CASE WHEN e.source_id = p.to_id THEN e.target_id ELSE e.source_id END AS TEXT
                   ), '') || ','
            FROM path p
            JOIN edges e ON e.source_id = p.to_id OR e.target_id = p.to_id
            JOIN nodes n2 ON n2.id = CASE WHEN e.source_id = p.to_id
                                      THEN e.target_id ELSE e.source_id END
            WHERE p.depth < ?3
              AND instr(p.visited, ',' || CAST(
                  CASE WHEN e.source_id = p.to_id THEN e.target_id ELSE e.source_id END AS TEXT
              ) || ',') = 0
        )
        SELECT route, depth
        FROM path
        WHERE to_id = (SELECT id FROM nodes WHERE label = ?2)
        ORDER BY depth
        LIMIT 1
        "#,
    )?;

    let result = stmt.query_row(rusqlite::params![from_label, to_label, max_depth], |row| {
        let route: String = row.get(0)?;
        Ok(route)
    });

    match result {
        Ok(route) => {
            let nodes: Vec<String> = route.split(" --> ").map(|s| s.to_string()).collect();
            debug!(
                "shortest_path: '{}' → '{}' encontrado: {} pasos",
                from_label,
                to_label,
                nodes.len()
            );
            Ok(Some(nodes))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            debug!(
                "shortest_path: '{}' → '{}' no encontrado (max_depth={})",
                from_label, to_label, max_depth
            );
            Ok(None)
        }
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::init_db(&conn).unwrap();

        // Insert test nodes
        conn.execute_batch(
            "INSERT INTO nodes (id, label, type) VALUES (1, 'Python', 'language');
             INSERT INTO nodes (id, label, type) VALUES (2, 'PostgreSQL', 'database');
             INSERT INTO nodes (id, label, type) VALUES (3, 'SQLAlchemy', 'library');
             INSERT INTO nodes (id, label, type) VALUES (4, 'Docker', 'tool');
             INSERT INTO edges (source_id, target_id, type, weight) VALUES (1, 3, 'uses', 0.9);
             INSERT INTO edges (source_id, target_id, type, weight) VALUES (3, 2, 'connects', 0.8);
             INSERT INTO edges (source_id, target_id, type, weight) VALUES (1, 4, 'runs_in', 0.7);
            ",
        )
        .unwrap();

        conn
    }

    #[test]
    fn test_expand_neighbors_direct() {
        let conn = setup_test_db();
        let neighbors = expand_neighbors(&conn, 1, 1).unwrap();
        assert!(neighbors.iter().any(|n| n.label == "SQLAlchemy"));
        assert!(neighbors.iter().any(|n| n.label == "Docker"));
    }

    #[test]
    fn test_expand_neighbors_depth_2() {
        let conn = setup_test_db();
        let neighbors = expand_neighbors(&conn, 1, 2).unwrap();
        assert!(neighbors.iter().any(|n| n.label == "PostgreSQL"));
    }

    #[test]
    fn test_shortest_path() {
        let conn = setup_test_db();
        let path = shortest_path(&conn, "Python", "PostgreSQL", 5).unwrap();
        assert!(path.is_some());
        let nodes = path.unwrap();
        assert!(nodes.contains(&"PostgreSQL".to_string()));
    }
}
