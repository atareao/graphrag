//! Force-directed layout algorithm for the concept-map TUI.
//!
//! This module implements a simplified Fruchterman-Reingold algorithm to
//! arrange graph nodes on a 2D canvas.  The algorithm simulates a physical
//! system in which:
//!
//! * **All nodes repel each other** (Coulomb's law: force ∝ k² / distance).
//! * **Connected nodes attract each other** (spring force: force ∝ distance² / k).
//! * **A cooling schedule** reduces movement over time so the layout converges.
//!
//! # Example (see tests section for runnable examples)
//!
//! ```text
//! let edges = [(0, 1), (1, 2), (2, 3)];
//! let layout = force_directed_layout(4, &edges, 800.0, 600.0, 100);
//! assert_eq!(layout.positions.len(), 4);
//! ```

use rand::RngExt;

/// A 2D position on the layout canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

/// The result of a force-directed layout computation.
///
/// Contains the final positions of every node as well as the canvas
/// dimensions that were used during simulation.
#[derive(Debug, Clone)]
pub struct Layout {
    pub positions: Vec<Position>,
    #[allow(dead_code)]
    pub width: f64,
    #[allow(dead_code)]
    pub height: f64,
}

/// Compute a force-directed layout using a simplified Fruchterman-Reingold
/// algorithm.
///
/// # Arguments
///
/// * `node_count` – Number of nodes to lay out.
/// * `edges`      – Slice of `(source_index, target_index)` pairs.  Indices
///   must be in `0..node_count`.
/// * `width`      – Canvas width.
/// * `height`     – Canvas height.
/// * `iterations` – Number of simulation iterations (higher → more stable).
///
/// # Returns
///
/// A [`Layout`] containing the final positions of all nodes.
///
/// # Panics
///
/// Panics if `node_count` is zero or if any edge index is out of bounds.
pub fn force_directed_layout(
    node_count: usize,
    edges: &[(usize, usize)],
    width: f64,
    height: f64,
    iterations: usize,
) -> Layout {
    assert!(node_count > 0, "node_count must be > 0");

    // Optimal distance between nodes (Fruchterman-Reingold k constant).
    let k = (width * height / node_count as f64).sqrt();

    // ------------------------------------------------------------------
    // Phase 1: Random initial placement with minimum separation
    // ------------------------------------------------------------------
    let mut rng = rand::rng();
    let min_sep_sq = (k * 0.2).powi(2);

    let mut positions: Vec<Position> = Vec::with_capacity(node_count);

    'next_node: for _ in 0..node_count {
        // Try up to 100 × node_count attempts to avoid overlap.
        for _ in 0..(100 * node_count.max(1)) {
            let candidate = Position {
                x: rng.random::<f64>() * width,
                y: rng.random::<f64>() * height,
            };

            let too_close = positions.iter().any(|p| {
                let dx = p.x - candidate.x;
                let dy = p.y - candidate.y;
                dx * dx + dy * dy < min_sep_sq
            });

            if !too_close {
                positions.push(candidate);
                continue 'next_node;
            }
        }
        // Fallback: place anywhere (rare).
        positions.push(Position {
            x: rng.random::<f64>() * width,
            y: rng.random::<f64>() * height,
        });
    }

    // ------------------------------------------------------------------
    // Phase 2: Iterative force simulation (Fruchterman-Reingold)
    // ------------------------------------------------------------------
    let mut disp = vec![Position { x: 0.0, y: 0.0 }; node_count];

    for iteration in 0..iterations {
        // Cooling factor: linear decrease from 1.0 → 0.0.
        let cooling = 1.0 - iteration as f64 / iterations.max(1) as f64;
        let max_disp = k * cooling;

        // Reset displacement accumulators.
        for d in &mut disp {
            d.x = 0.0;
            d.y = 0.0;
        }

        // --- Repulsive forces (every pair of nodes) ---
        for i in 0..node_count {
            for j in (i + 1)..node_count {
                let dx = positions[j].x - positions[i].x;
                let dy = positions[j].y - positions[i].y;
                let dist = (dx * dx + dy * dy).sqrt().max(1.0);

                let force = (k * k) / dist;
                let fx = (dx / dist) * force;
                let fy = (dy / dist) * force;

                disp[i].x -= fx;
                disp[i].y -= fy;
                disp[j].x += fx;
                disp[j].y += fy;
            }
        }

        // --- Attractive forces (along edges) ---
        for &(src, tgt) in edges {
            debug_assert!(
                src < node_count && tgt < node_count,
                "edge index out of bounds: ({src}, {tgt}) with {node_count} nodes"
            );

            let dx = positions[tgt].x - positions[src].x;
            let dy = positions[tgt].y - positions[src].y;
            let dist = (dx * dx + dy * dy).sqrt().max(1.0);

            let force = (dist * dist) / k;
            let fx = (dx / dist) * force;
            let fy = (dy / dist) * force;

            disp[src].x += fx;
            disp[src].y += fy;
            disp[tgt].x -= fx;
            disp[tgt].y -= fy;
        }

        // --- Apply displacements with cooling and clamping ---
        for i in 0..node_count {
            let d = (disp[i].x * disp[i].x + disp[i].y * disp[i].y).sqrt();
            if d > 0.0 {
                let scale = max_disp.min(d) / d;
                positions[i].x += disp[i].x * scale;
                positions[i].y += disp[i].y * scale;
            }

            // Clamp to canvas bounds with a small padding.
            let padding = 5.0;
            positions[i].x = positions[i].x.clamp(padding, width - padding);
            positions[i].y = positions[i].y.clamp(padding, height - padding);
        }
    }

    Layout {
        positions,
        width,
        height,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: compute Euclidean distance between two positions.
    fn distance(a: &Position, b: &Position) -> f64 {
        ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
    }

    // -----------------------------------------------------------------------
    // test_layout_returns_correct_count
    // -----------------------------------------------------------------------

    #[test]
    fn test_layout_returns_correct_count() {
        let edges = [(0, 1), (1, 2), (2, 3), (3, 4)];
        let layout = force_directed_layout(5, &edges, 800.0, 600.0, 50);
        assert_eq!(
            layout.positions.len(),
            5,
            "should return exactly node_count positions"
        );
    }

    // -----------------------------------------------------------------------
    // test_layout_positions_within_bounds
    // -----------------------------------------------------------------------

    #[test]
    fn test_layout_positions_within_bounds() {
        let edges: Vec<(usize, usize)> = (0..9).map(|i| (i, i + 1)).collect();
        let w = 640.0;
        let h = 480.0;
        let layout = force_directed_layout(10, &edges, w, h, 100);

        for (i, pos) in layout.positions.iter().enumerate() {
            assert!(
                pos.x >= 0.0 && pos.x <= w,
                "node {i}: x={} out of bounds [0, {w}]",
                pos.x
            );
            assert!(
                pos.y >= 0.0 && pos.y <= h,
                "node {i}: y={} out of bounds [0, {h}]",
                pos.y
            );
        }
    }

    // -----------------------------------------------------------------------
    // test_layout_converges
    // -----------------------------------------------------------------------

    #[test]
    fn test_layout_converges() {
        // Star graph: node 0 connected to all others.
        let node_count = 20;
        let edges: Vec<(usize, usize)> = (1..node_count).map(|i| (0, i)).collect();

        let layout = force_directed_layout(node_count, &edges, 800.0, 600.0, 200);

        // After many iterations the layout should not have diverged:
        // all positions must be within the canvas bounds (padding = 5.0).
        for (i, pos) in layout.positions.iter().enumerate() {
            assert!(
                pos.x >= 0.0 && pos.x <= 800.0,
                "node {i}: x={} out of bounds after 200 iterations",
                pos.x
            );
            assert!(
                pos.y >= 0.0 && pos.y <= 600.0,
                "node {i}: y={} out of bounds after 200 iterations",
                pos.y
            );
        }

        // The centre of mass should be roughly near the middle of the canvas.
        let cx = layout.positions.iter().map(|p| p.x).sum::<f64>() / node_count as f64;
        let cy = layout.positions.iter().map(|p| p.y).sum::<f64>() / node_count as f64;
        let w = 800.0;
        let h = 600.0;

        assert!(
            cx > w * 0.2 && cx < w * 0.8,
            "centre of mass x={cx:.1} is far from canvas centre (should be ~{})",
            w / 2.0
        );
        assert!(
            cy > h * 0.2 && cy < h * 0.8,
            "centre of mass y={cy:.1} is far from canvas centre (should be ~{})",
            h / 2.0
        );
    }

    // -----------------------------------------------------------------------
    // test_layout_no_overlap
    // -----------------------------------------------------------------------

    #[test]
    fn test_layout_no_overlap() {
        let node_count = 15;
        let edges: Vec<(usize, usize)> = (0..node_count - 1).map(|i| (i, i + 1)).collect();
        let layout = force_directed_layout(node_count, &edges, 600.0, 600.0, 200);

        // Compute the minimum pairwise distance.
        let min_dist = (0..node_count)
            .flat_map(|i| ((i + 1)..node_count).map(move |j| (i, j)))
            .map(|(i, j)| distance(&layout.positions[i], &layout.positions[j]))
            .fold(f64::MAX, f64::min);

        // Even with clamping at the canvas edges, the repulsion force
        // ensures nodes don't occupy *exactly* the same point.  A tiny
        // tolerance accounts for floating-point corner cases.
        assert!(
            min_dist > 1e-6,
            "two nodes are at essentially the same position (min_dist={min_dist:e})"
        );
    }

    // -----------------------------------------------------------------------
    // test_layout_connected_nodes
    // -----------------------------------------------------------------------

    #[test]
    fn test_layout_connected_nodes() {
        // Create a graph with two clusters:
        //   Cluster A: 6 nodes connected in a chain (0-1-2-3-4-5)
        //   Cluster B: 6 nodes connected in a chain (6-7-8-9-10-11)
        //   One weak cross-edge: 3 ↔ 9
        let node_count = 12;
        let mut edges: Vec<(usize, usize)> = (0..5).map(|i| (i, i + 1)).collect();
        edges.extend((6..11).map(|i| (i, i + 1)));
        edges.push((3, 9)); // cross-cluster bridge

        let layout = force_directed_layout(node_count, &edges, 800.0, 600.0, 150);

        // Collect connected and unconnected distances.
        let mut connected_dists: Vec<f64> = Vec::new();
        let mut unconnected_dists: Vec<f64> = Vec::new();

        for i in 0..node_count {
            for j in (i + 1)..node_count {
                let d = distance(&layout.positions[i], &layout.positions[j]);
                let is_connected = edges
                    .iter()
                    .any(|(a, b)| (*a == i && *b == j) || (*a == j && *b == i));
                if is_connected {
                    connected_dists.push(d);
                } else {
                    unconnected_dists.push(d);
                }
            }
        }

        // Statistical test: the *mean* distance between connected nodes
        // should be less than between unconnected nodes.
        let mean_connected = connected_dists.iter().sum::<f64>() / connected_dists.len() as f64;
        let mean_unconnected =
            unconnected_dists.iter().sum::<f64>() / unconnected_dists.len() as f64;

        assert!(
            mean_connected < mean_unconnected,
            "connected nodes should be closer on average: \
             mean_connected={mean_connected:.2}, mean_unconnected={mean_unconnected:.2}"
        );
    }

    // -----------------------------------------------------------------------
    // test_layout_empty_edges
    // -----------------------------------------------------------------------

    #[test]
    fn test_layout_empty_edges() {
        // With no edges, repulsion should spread nodes apart.
        let layout = force_directed_layout(5, &[], 500.0, 500.0, 50);
        assert_eq!(layout.positions.len(), 5);
        for pos in &layout.positions {
            assert!(pos.x >= 0.0 && pos.x <= 500.0);
            assert!(pos.y >= 0.0 && pos.y <= 500.0);
        }
    }

    // -----------------------------------------------------------------------
    // test_layout_single_node
    // -----------------------------------------------------------------------

    #[test]
    fn test_layout_single_node() {
        let layout = force_directed_layout(1, &[], 100.0, 100.0, 10);
        assert_eq!(layout.positions.len(), 1);
        let pos = &layout.positions[0];
        assert!(pos.x >= 0.0 && pos.x <= 100.0);
        assert!(pos.y >= 0.0 && pos.y <= 100.0);
    }
}
