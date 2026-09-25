//! Interactive concept-map TUI.
//!
//! Provides a terminal user interface (TUI) that renders a force-directed
//! graph of notes, entities, and tags, navigable via keyboard and mouse.
//!
//! # Usage
//!
//! Call [`run_tui`] with a pre-built [`AppState`]:
//!
//! ```ignore
//! let state = AppState { nodes, edges, positions, … };
//! run_tui(state)?;
//! ```

use std::io::stdout;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::{Line as TextLine, Span},
    widgets::{
        canvas::{Canvas, Line, Rectangle},
        Block, Borders, Paragraph,
    },
    Terminal,
};

use crate::map::layout::Position;
use crate::map::{MapEdge, MapNode};

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

/// All the data and UI state needed to render and interact with the concept
/// map.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Nodes loaded from the database.
    pub nodes: Vec<MapNode>,
    /// Edges between nodes.
    pub edges: Vec<MapEdge>,
    /// 2-D layout positions for each node (parallel to `nodes`).
    pub positions: Vec<Position>,
    /// Index of the currently highlighted / selected node, if any.
    pub selected_idx: Option<usize>,
    /// Optional type filter: `None` = all, `Some("note")`, `Some("entity")`,
    /// `Some("tag")`.
    pub filter_type: Option<String>,
    /// Active search query text.
    pub search_query: String,
    /// Whether the user is currently typing a search query.
    pub search_active: bool,
    /// Horizontal viewport offset (panning).
    pub offset_x: f64,
    /// Vertical viewport offset (panning).
    pub offset_y: f64,
    /// Graph depth used when loading subgraphs.
    #[allow(dead_code)]
    pub depth: i32,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            positions: Vec::new(),
            selected_idx: None,
            filter_type: None,
            search_query: String::new(),
            search_active: false,
            offset_x: 0.0,
            offset_y: 0.0,
            depth: 2,
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: world bounds from positions
// ---------------------------------------------------------------------------

/// Return the effective world width and height of the layout.
///
/// Falls back to `(800.0, 600.0)` when there are no positions.
fn world_bounds(positions: &[Position]) -> (f64, f64) {
    if positions.is_empty() {
        return (800.0, 600.0);
    }
    let w = positions
        .iter()
        .map(|p| p.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let h = positions
        .iter()
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max);
    // Guard against degenerate cases where all nodes are at 0,0.
    (w.max(100.0), h.max(100.0))
}

// ---------------------------------------------------------------------------
// Helpers: selection & visibility
// ---------------------------------------------------------------------------

/// Find the nearest node in a given direction from the currently selected
/// node.  If no node is selected, returns the first visible node.
pub fn nearest_node_in_direction(state: &AppState, dx: f64, dy: f64) -> Option<usize> {
    let visible: Vec<usize> = visible_nodes(state);
    if visible.is_empty() {
        return None;
    }

    let sel = state.selected_idx.filter(|&i| visible.contains(&i));

    let (sel_idx, sel_pos) = match sel {
        Some(i) => (i, &state.positions[i]),
        None => return visible.first().copied(),
    };

    let target_angle = dy.atan2(dx);

    let mut best_idx = None;
    let mut best_diff = std::f64::consts::TAU; // 2π

    for &i in &visible {
        if i == sel_idx {
            continue;
        }
        let p = &state.positions[i];
        let a = (p.y - sel_pos.y).atan2(p.x - sel_pos.x);
        let mut diff = (a - target_angle).abs();
        if diff > std::f64::consts::PI {
            diff = std::f64::consts::TAU - diff;
        }
        if diff < best_diff {
            best_diff = diff;
            best_idx = Some(i);
        }
    }

    best_idx
}

/// Find the node whose rectangle contains the given canvas coordinates, if
/// any.
///
/// `canvas_x` / `canvas_y` are in the Canvas logical coordinate system
/// (i.e., after the same transform applied in [`render_canvas`]).
pub fn node_at_position(
    state: &AppState,
    canvas_x: f64,
    canvas_y: f64,
    canvas_w: f64,
    canvas_h: f64,
) -> Option<usize> {
    let (world_w, world_h) = world_bounds(&state.positions);
    for (i, pos) in state.positions.iter().enumerate() {
        let label_len = state.nodes[i].label.len() as f64;
        let node_w = (label_len * 0.6).max(4.0);
        let node_h = 3.0;
        let sx = pos.x * (canvas_w / world_w) + state.offset_x;
        let sy = pos.y * (canvas_h / world_h) + state.offset_y;
        if canvas_x >= sx && canvas_x <= sx + node_w && canvas_y >= sy && canvas_y <= sy + node_h {
            return Some(i);
        }
    }
    None
}

/// Return the indices of nodes that match the current filter.
pub fn visible_nodes(state: &AppState) -> Vec<usize> {
    let filter = state.filter_type.as_deref();
    state
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| match filter {
            None => true,
            Some(ft) => n.type_.eq_ignore_ascii_case(ft),
        })
        .map(|(i, _)| i)
        .collect()
}

/// Return the indices of edges whose *both* endpoints are visible.
#[allow(dead_code)]
pub fn visible_edges(state: &AppState) -> Vec<usize> {
    let visible: Vec<i64> = visible_nodes(state)
        .iter()
        .map(|&i| state.nodes[i].id)
        .collect();
    state
        .edges
        .iter()
        .enumerate()
        .filter(|(_, e)| visible.contains(&e.source_id) && visible.contains(&e.target_id))
        .map(|(i, _)| i)
        .collect()
}

/// Return the color for a node based on its type, using a bright variant when
/// the node is selected.
fn node_color(type_: &str, selected: bool) -> Color {
    match (type_.to_lowercase().as_str(), selected) {
        ("note", false) => Color::Blue,
        ("note", true) => Color::LightBlue,
        ("entity", false) => Color::Green,
        ("entity", true) => Color::LightGreen,
        ("tag", false) => Color::Yellow,
        ("tag", true) => Color::LightYellow,
        (_, false) => Color::White,
        (_, true) => Color::LightMagenta,
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Build the [`Canvas`] widget for the graph pane.
///
/// Takes owned data so the paint closure can capture it without borrowing
/// constraints.
#[allow(clippy::too_many_arguments)]
fn render_canvas<'a>(
    nodes: Vec<MapNode>,
    edges: Vec<MapEdge>,
    positions: Vec<Position>,
    selected_idx: Option<usize>,
    filter_type: Option<String>,
    search_query: String,
    search_active: bool,
    offset_x: f64,
    offset_y: f64,
    canvas_area: Rect,
) -> Canvas<'a, impl Fn(&mut ratatui::widgets::canvas::Context)> {
    let canvas_w = canvas_area.width.max(1) as f64;
    let canvas_h = canvas_area.height.max(1) as f64;
    let (world_w, world_h) = world_bounds(&positions);

    // Pre-compute visibility.
    let filter_deref = filter_type.as_deref();
    let visible_idx: Vec<usize> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| match filter_deref {
            None => true,
            Some(ft) => n.type_.eq_ignore_ascii_case(ft),
        })
        .map(|(i, _)| i)
        .collect();

    let visible_ids: Vec<i64> = visible_idx.iter().map(|&i| nodes[i].id).collect();
    let visible_edge_idx: Vec<usize> = edges
        .iter()
        .enumerate()
        .filter(|(_, e)| visible_ids.contains(&e.source_id) && visible_ids.contains(&e.target_id))
        .map(|(i, _)| i)
        .collect();

    let search_text = search_query.to_lowercase();
    let has_search = search_active && !search_text.is_empty();

    Canvas::default()
        .x_bounds([0.0, canvas_w])
        .y_bounds([0.0, canvas_h])
        .marker(Marker::HalfBlock)
        .background_color(Color::Black)
        .paint(move |ctx| {
            // ---- Edges ----
            for &ei in &visible_edge_idx {
                let edge = &edges[ei];
                let src_idx = nodes.iter().position(|n| n.id == edge.source_id);
                let tgt_idx = nodes.iter().position(|n| n.id == edge.target_id);
                let (Some(si), Some(ti)) = (src_idx, tgt_idx) else {
                    continue;
                };
                let src_pos = &positions[si];
                let tgt_pos = &positions[ti];

                let x1 = src_pos.x * (canvas_w / world_w) + offset_x;
                let y1 = src_pos.y * (canvas_h / world_h) + offset_y;
                let x2 = tgt_pos.x * (canvas_w / world_w) + offset_x;
                let y2 = tgt_pos.y * (canvas_h / world_h) + offset_y;

                let on_screen =
                    (x1 >= -5.0 && x1 <= canvas_w + 5.0) || (x2 >= -5.0 && x2 <= canvas_w + 5.0);
                if !on_screen {
                    continue;
                }

                ctx.draw(&Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    color: Color::Gray,
                });

                let mid_x = (x1 + x2) / 2.0;
                let mid_y = (y1 + y2) / 2.0;
                let dist = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt();
                if dist > 8.0 && !edge.type_.is_empty() {
                    ctx.print(mid_x, mid_y, edge.type_.clone());
                }
            }

            // ---- Nodes ----
            for &ni in &visible_idx {
                let node = &nodes[ni];
                let pos = &positions[ni];

                let node_w = (node.label.len() as f64 * 0.6).max(4.0);
                let node_h = 3.0;

                let sx = pos.x * (canvas_w / world_w) + offset_x;
                let sy = pos.y * (canvas_h / world_h) + offset_y;

                if sx + node_w < 0.0 || sx > canvas_w || sy + node_h < 0.0 || sy > canvas_h {
                    continue;
                }

                let sel = selected_idx == Some(ni);
                let color = node_color(&node.type_, sel);
                let search_match = has_search && node.label.to_lowercase().contains(&search_text);

                if sel || search_match {
                    let border_color = if search_match {
                        Color::Magenta
                    } else {
                        Color::White
                    };
                    ctx.draw(&Rectangle {
                        x: sx - 0.5,
                        y: sy - 0.5,
                        width: node_w + 1.0,
                        height: node_h + 1.0,
                        color: border_color,
                    });
                }

                ctx.draw(&Rectangle {
                    x: sx,
                    y: sy,
                    width: node_w,
                    height: node_h,
                    color,
                });

                let text_color = if search_match { Color::Magenta } else { color };
                let label_line = TextLine::from(vec![Span::styled(
                    node.label.clone(),
                    Style::default().fg(text_color),
                )]);
                ctx.print(sx + 0.5, sy + 0.5, label_line);
            }

            // ---- Empty state ----
            if visible_idx.is_empty() {
                let msg = "No nodes to display".to_string();
                let cx = canvas_w / 2.0 - (msg.len() as f64 * 0.3);
                let cy = canvas_h / 2.0;
                ctx.print(
                    cx,
                    cy,
                    TextLine::from(vec![Span::styled(msg, Style::default().fg(Color::Gray))]),
                );
            }
        })
}

/// Build the details [`Paragraph`] widget.
fn render_details<'a>(state: &'a AppState) -> Paragraph<'a> {
    match state.selected_idx {
        None => Paragraph::new(TextLine::from(vec![Span::styled(
            "No node selected",
            Style::default().fg(Color::Gray),
        )])),
        Some(idx) => {
            let node = &state.nodes[idx];
            let pos = &state.positions[idx];

            // Node info lines.
            let mut lines: Vec<TextLine> = Vec::new();
            lines.push(TextLine::from(vec![
                Span::styled("Node: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&node.label),
            ]));
            lines.push(TextLine::from(vec![
                Span::styled("Type: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&node.type_),
            ]));
            lines.push(TextLine::from(vec![
                Span::styled("ID: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("{}", node.id)),
            ]));
            lines.push(TextLine::from(vec![
                Span::styled("Pos: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("({:.1}, {:.1})", pos.x, pos.y)),
            ]));

            // Separator.
            lines.push(TextLine::from(Span::raw("─".repeat(20))));

            // Neighbors.
            lines.push(TextLine::from(vec![
                Span::styled("Neighbors:", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(""),
            ]));

            for edge in &state.edges {
                let is_source = edge.source_id == node.id;
                let is_target = edge.target_id == node.id;
                if !is_source && !is_target {
                    continue;
                }
                let neighbor_id = if is_source {
                    edge.target_id
                } else {
                    edge.source_id
                };
                // Look up neighbor label.
                let neighbor_label = state
                    .nodes
                    .iter()
                    .find(|n| n.id == neighbor_id)
                    .map(|n| n.label.as_str())
                    .unwrap_or("?");
                let direction = if is_source { "→" } else { "←" };
                lines.push(TextLine::from(vec![
                    Span::raw(format!("  {neighbor_label} {direction} {}", edge.type_)),
                    Span::styled(
                        format!(" (w={:.2})", edge.weight),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }

            Paragraph::new(lines)
        }
    }
    .block(Block::default().borders(Borders::ALL).title(" Details "))
}

/// Build the status bar [`Paragraph`] widget.
fn render_status<'a>(state: &AppState) -> Paragraph<'a> {
    let total = state.nodes.len();
    let visible = visible_nodes(state).len();

    // Node count.
    let node_info = format!("{visible}/{total} nodes");

    // Filter info.
    let filter_info = match state.filter_type.as_deref() {
        None => "filter: all",
        Some(ft) => {
            // Use short labels for the three allowed types.
            match ft {
                "note" => "filter: notes only",
                "entity" => "filter: entities only",
                "tag" => "filter: tags only",
                other => {
                    return Paragraph::new(TextLine::from(Span::raw(format!("filter: {other}"))))
                }
            }
        }
    };

    // Search info.
    let search_info = if state.search_active && !state.search_query.is_empty() {
        format!("/{}", state.search_query)
    } else if state.search_active {
        "/_".to_string()
    } else {
        String::new()
    };

    // Keybinding hints.
    let hints = "↑↓←→ select  ↵ details  t filter  / search  q quit";

    let mut parts: Vec<String> = vec![node_info, filter_info.to_string()];
    if !search_info.is_empty() {
        parts.push(search_info);
    }
    parts.push(hints.to_string());

    Paragraph::new(TextLine::from(Span::styled(
        parts.join("  │  "),
        Style::default().fg(Color::White),
    )))
    .style(Style::default().bg(Color::DarkGray))
}

// ---------------------------------------------------------------------------
// Event loop
// ---------------------------------------------------------------------------

/// Run the interactive concept-map TUI.
///
/// Sets up the terminal, enters the event loop, and restores the terminal on
/// exit.
///
/// # Errors
///
/// Returns an error if terminal setup or the event loop fails unexpectedly.
pub fn run_tui(mut state: AppState) -> Result<()> {
    let mut stdout = stdout();

    // --- Terminal setup ---
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;
    // Enable mouse capture so we can handle click events.
    execute!(stdout, crossterm::event::EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    terminal.hide_cursor()?;

    // --- Main event loop ---
    let result = event_loop(&mut terminal, &mut state);

    // --- Terminal teardown ---
    terminal.show_cursor()?;
    execute!(
        terminal.backend_mut(),
        crossterm::event::DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    disable_raw_mode()?;

    result
}

/// The inner event loop, factored out so `run_tui` can always perform proper
/// cleanup on both success and error.
fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    state: &mut AppState,
) -> Result<()> {
    loop {
        // --- Render ---
        terminal.draw(|f| {
            let area = f.area();

            // Vertical layout: title | main | status
            let vert = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Min(0),
                    Constraint::Length(1),
                ])
                .split(area);

            // Title bar.
            let title_block = Block::default()
                .title(" Concept Map ")
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Cyan));
            f.render_widget(title_block, vert[0]);

            // Main area: canvas | detail panel
            let horiz = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
                .split(vert[1]);

            // Graph canvas.
            let canvas_widget = render_canvas(
                state.nodes.clone(),
                state.edges.clone(),
                state.positions.clone(),
                state.selected_idx,
                state.filter_type.clone(),
                state.search_query.clone(),
                state.search_active,
                state.offset_x,
                state.offset_y,
                horiz[0],
            );
            f.render_widget(canvas_widget, horiz[0]);

            // Details panel.
            let details_widget = render_details(state);
            f.render_widget(details_widget, horiz[1]);

            // Status bar.
            let status_widget = render_status(state);
            f.render_widget(status_widget, vert[2]);
        })?;

        // --- Event handling ---
        let event = event::read()?;

        match event {
            Event::Key(key_event) => {
                if key_event.kind == KeyEventKind::Press && handle_key(state, key_event.code) {
                    break; // quit
                }
            }
            Event::Mouse(mouse_event) => {
                if mouse_event.kind == MouseEventKind::Down(crossterm::event::MouseButton::Left) {
                    handle_mouse(state, mouse_event.column, mouse_event.row, terminal);
                }
            }
            Event::Resize(..) => {
                // Just re-render on resize — handled by next draw call.
            }
            _ => {}
        }
    }

    Ok(())
}

/// Handle a key-press event.  Returns `true` if the application should quit.
fn handle_key(state: &mut AppState, code: KeyCode) -> bool {
    if state.search_active {
        return handle_search_key(state, code);
    }

    match code {
        KeyCode::Char('q') | KeyCode::Esc => true,
        KeyCode::Up | KeyCode::Char('k') => {
            state.selected_idx = nearest_node_in_direction(state, 0.0, 1.0);
            false
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.selected_idx = nearest_node_in_direction(state, 0.0, -1.0);
            false
        }
        KeyCode::Left | KeyCode::Char('h') => {
            state.selected_idx = nearest_node_in_direction(state, -1.0, 0.0);
            false
        }
        KeyCode::Right | KeyCode::Char('l') => {
            state.selected_idx = nearest_node_in_direction(state, 1.0, 0.0);
            false
        }
        KeyCode::Enter => {
            // Enter already selects — this is a no-op for now (details panel
            // already shows the selected node).  Could be extended to "open"
            // the node.
            false
        }
        KeyCode::Char('t') => {
            cycle_filter(state);
            false
        }
        KeyCode::Char('/') => {
            state.search_active = true;
            state.search_query.clear();
            false
        }
        _ => false,
    }
}

/// Handle a key-press while search mode is active.
fn handle_search_key(state: &mut AppState, code: KeyCode) -> bool {
    match code {
        KeyCode::Esc => {
            state.search_active = false;
            state.search_query.clear();
            false
        }
        KeyCode::Enter => {
            state.search_active = false;
            // Keep the query for highlighting.
            false
        }
        KeyCode::Backspace => {
            state.search_query.pop();
            false
        }
        KeyCode::Char(c) if c.is_ascii_graphic() || c == ' ' => {
            state.search_query.push(c);
            false
        }
        _ => false,
    }
}

/// Handle a mouse click — attempt to select the node under the cursor.
fn handle_mouse(
    state: &mut AppState,
    col: u16,
    row: u16,
    terminal: &Terminal<CrosstermBackend<std::io::Stdout>>,
) {
    // We need the canvas-area rectangle to map screen coords → canvas coords.
    // Grab the terminal's current size and build a full-area Rect.
    let size = terminal
        .size()
        .ok()
        .unwrap_or_else(|| ratatui::layout::Size::new(80, 24));
    let area = Rect::new(0, 0, size.width, size.height);

    // Recompute the same layout as in `event_loop`.
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);
    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(vert[1]);

    let canvas_area = horiz[0];
    if col < canvas_area.left()
        || col >= canvas_area.right()
        || row < canvas_area.top()
        || row >= canvas_area.bottom()
    {
        return;
    }

    // Map terminal cell coordinates to canvas logical coordinates.
    let canvas_w = canvas_area.width.max(1) as f64;
    let canvas_h = canvas_area.height.max(1) as f64;
    let cx = (col - canvas_area.left()) as f64;
    let cy = (row - canvas_area.top()) as f64;

    if let Some(idx) = node_at_position(state, cx, cy, canvas_w, canvas_h) {
        state.selected_idx = Some(idx);
    }
}

/// Cycle the filter through: all → note → entity → tag → all …
fn cycle_filter(state: &mut AppState) {
    let next = match state.filter_type.as_deref() {
        None => Some("note"),
        Some("note") => Some("entity"),
        Some("entity") => Some("tag"),
        Some("tag") => None,
        Some(_other) => None,
    };
    state.filter_type = next.map(String::from);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> AppState {
        AppState {
            nodes: vec![
                MapNode {
                    id: 1,
                    label: "Python".into(),
                    type_: "note".into(),
                },
                MapNode {
                    id: 2,
                    label: "Rust".into(),
                    type_: "entity".into(),
                },
                MapNode {
                    id: 3,
                    label: "Tokio".into(),
                    type_: "entity".into(),
                },
                MapNode {
                    id: 4,
                    label: "tutorial".into(),
                    type_: "tag".into(),
                },
                MapNode {
                    id: 5,
                    label: "Async".into(),
                    type_: "note".into(),
                },
            ],
            edges: vec![
                MapEdge {
                    source_id: 1,
                    target_id: 2,
                    type_: "related".into(),
                    weight: 0.8,
                    context: None,
                },
                MapEdge {
                    source_id: 2,
                    target_id: 3,
                    type_: "depends".into(),
                    weight: 0.9,
                    context: None,
                },
                MapEdge {
                    source_id: 1,
                    target_id: 4,
                    type_: "tagged".into(),
                    weight: 0.5,
                    context: None,
                },
                MapEdge {
                    source_id: 5,
                    target_id: 2,
                    type_: "related".into(),
                    weight: 0.7,
                    context: None,
                },
            ],
            positions: vec![
                Position { x: 100.0, y: 200.0 },
                Position { x: 300.0, y: 150.0 },
                Position { x: 500.0, y: 400.0 },
                Position { x: 200.0, y: 500.0 },
                Position { x: 400.0, y: 100.0 },
            ],
            selected_idx: None,
            filter_type: None,
            search_query: String::new(),
            search_active: false,
            offset_x: 0.0,
            offset_y: 0.0,
            depth: 2,
        }
    }

    // -----------------------------------------------------------------------
    // test_app_state_initialization
    // -----------------------------------------------------------------------

    #[test]
    fn test_app_state_initialization() {
        let state: AppState = AppState::default();
        assert!(state.nodes.is_empty());
        assert!(state.edges.is_empty());
        assert!(state.positions.is_empty());
        assert_eq!(state.selected_idx, None);
        assert_eq!(state.filter_type, None);
        assert!(state.search_query.is_empty());
        assert!(!state.search_active);
        assert_eq!(state.offset_x, 0.0);
        assert_eq!(state.offset_y, 0.0);
        assert_eq!(state.depth, 2);
    }

    // -----------------------------------------------------------------------
    // test_visible_nodes_no_filter
    // -----------------------------------------------------------------------

    #[test]
    fn test_visible_nodes_no_filter() {
        let state = test_state();
        let visible = visible_nodes(&state);
        assert_eq!(visible.len(), 5, "all 5 nodes should be visible");
    }

    // -----------------------------------------------------------------------
    // test_visible_nodes_filter_notes
    // -----------------------------------------------------------------------

    #[test]
    fn test_visible_nodes_filter_notes() {
        let mut state = test_state();
        state.filter_type = Some("note".into());
        let visible = visible_nodes(&state);
        assert_eq!(visible.len(), 2, "only 2 note-type nodes");
        for &i in &visible {
            assert_eq!(state.nodes[i].type_, "note");
        }
    }

    // -----------------------------------------------------------------------
    // test_visible_edges_filtered
    // -----------------------------------------------------------------------

    #[test]
    fn test_visible_edges_filtered() {
        let mut state = test_state();
        // Filter to "note" only: nodes 1 (Python) and 5 (Async).
        state.filter_type = Some("note".into());
        let visible = visible_edges(&state);
        // Edges whose both endpoints are notes:
        //   None — because no edge connects two notes directly.
        // Edge 1→2 (Python→Rust) has Rust which is entity → filtered out.
        // So the result should be empty.
        assert_eq!(visible.len(), 0, "no edges between two notes");
    }

    #[test]
    fn test_visible_edges_with_entity_filter() {
        let mut state = test_state();
        // Filter to "entity" only: nodes 2 (Rust), 3 (Tokio).
        state.filter_type = Some("entity".into());
        let visible = visible_edges(&state);
        // Edge 2→3 (Rust→Tokio) has both endpoints as entities → visible.
        assert_eq!(visible.len(), 1, "one edge between two entities");
        assert_eq!(state.edges[visible[0]].type_, "depends");
    }

    // -----------------------------------------------------------------------
    // test_search_matching
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_matching() {
        let state = test_state();
        // The search_active flag alone doesn't filter nodes — it's used for
        // highlighting.  We test that visible_nodes returns all when filter
        // is None, and that case-insensitive matching works on labels.
        let search_query = "rust".to_lowercase();
        let matched: Vec<&str> = state
            .nodes
            .iter()
            .filter(|n| n.label.to_lowercase().contains(&search_query))
            .map(|n| n.label.as_str())
            .collect();
        assert_eq!(matched, vec!["Rust"]);
    }

    // -----------------------------------------------------------------------
    // test_search_no_match
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_no_match() {
        let state = test_state();
        let search_query = "nonexistent".to_lowercase();
        let matched: Vec<&str> = state
            .nodes
            .iter()
            .filter(|n| n.label.to_lowercase().contains(&search_query))
            .map(|n| n.label.as_str())
            .collect();
        assert!(matched.is_empty(), "no labels should match 'nonexistent'");
    }

    // -----------------------------------------------------------------------
    // test_cycle_filter
    // -----------------------------------------------------------------------

    #[test]
    fn test_cycle_filter() {
        let mut state = test_state();

        // Start: all
        assert_eq!(state.filter_type, None);

        cycle_filter(&mut state);
        assert_eq!(state.filter_type.as_deref(), Some("note"));

        cycle_filter(&mut state);
        assert_eq!(state.filter_type.as_deref(), Some("entity"));

        cycle_filter(&mut state);
        assert_eq!(state.filter_type.as_deref(), Some("tag"));

        cycle_filter(&mut state);
        assert_eq!(state.filter_type, None, "cycles back to all");
    }

    // -----------------------------------------------------------------------
    // test_nearest_node_in_direction
    // -----------------------------------------------------------------------

    #[test]
    fn test_nearest_node_in_direction_no_selection() {
        let state = test_state();
        let result = nearest_node_in_direction(&state, 1.0, 0.0);
        // No node selected → returns first visible node (index 0).
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_nearest_node_in_direction_with_selection() {
        let mut state = test_state();
        state.selected_idx = Some(0); // Python at (100, 200)

        // Direction: right (dx=1, dy=0)
        let right = nearest_node_in_direction(&state, 1.0, 0.0);
        // Python (100,200) → candidates with x > 100:
        //   Rust  (300,150) angle ~ -0.17 rad
        //   tutorial (200,500) angle ~ 1.25 rad
        //   Async (400,100) angle ~ -0.46 rad
        //   Tokio (500,400) angle ~ 0.56 rad
        // Closest to 0 rad = Rust (angle ~ -0.17 → diff = 0.17)
        assert_eq!(right, Some(1), "expected Rust (idx 1) to be nearest right");
    }

    // -----------------------------------------------------------------------
    // test_handle_search_key
    // -----------------------------------------------------------------------

    #[test]
    fn test_handle_search_key_escape() {
        let mut state = test_state();
        state.search_active = true;
        state.search_query = "test".into();

        let quit = handle_search_key(&mut state, KeyCode::Esc);
        assert!(!quit, "Esc should not quit during search");
        assert!(!state.search_active, "search should be cancelled");
        assert!(state.search_query.is_empty(), "query should be cleared");
    }

    #[test]
    fn test_handle_search_key_enter() {
        let mut state = test_state();
        state.search_active = true;
        state.search_query = "rust".into();

        let quit = handle_search_key(&mut state, KeyCode::Enter);
        assert!(!quit, "Enter should not quit");
        assert!(!state.search_active, "search mode should end");
        assert_eq!(state.search_query, "rust", "query should be preserved");
    }

    #[test]
    fn test_handle_search_key_backspace() {
        let mut state = test_state();
        state.search_active = true;
        state.search_query = "rust".into();

        handle_search_key(&mut state, KeyCode::Backspace);
        assert_eq!(state.search_query, "rus");
    }

    #[test]
    fn test_handle_search_key_char() {
        let mut state = test_state();
        state.search_active = true;

        handle_search_key(&mut state, KeyCode::Char('a'));
        assert_eq!(state.search_query, "a");

        handle_search_key(&mut state, KeyCode::Char('b'));
        assert_eq!(state.search_query, "ab");
    }
}
