# concept-map-tui Specification

## Purpose
Provides an interactive terminal-based concept map explorer that lets users visually navigate the knowledge graph, inspect nodes, and discover relationships without external tools.

## Requirements

### Requirement: CLI command `graphrag map`

The system SHALL provide a `graphrag map` CLI command that opens an interactive TUI for exploring the knowledge graph.

#### Scenario: Opens TUI from CLI
- **WHEN** user runs `graphrag map graph.db`
- **THEN** the system opens an interactive TUI showing the knowledge graph

#### Scenario: Opens TUI with starting node
- **WHEN** user runs `graphrag map --from Python graph.db`
- **THEN** the system opens the TUI centered on the node labeled "Python"

#### Scenario: Opens TUI with custom depth
- **WHEN** user runs `graphrag map --from Python --depth 3 graph.db`
- **THEN** the system opens the TUI showing nodes up to 3 hops from "Python"

#### Scenario: Error on non-existent starting node
- **WHEN** user runs `graphrag map --from NonExistent graph.db`
- **THEN** the system prints an error message and exits without opening the TUI

#### Scenario: Error on non-existent database
- **WHEN** user runs `graphrag map nonexistent.db`
- **THEN** the system prints an error message and exits

### Requirement: Graph visualization in TUI

The TUI SHALL display nodes as labeled boxes and edges as directed lines with relationship labels.

#### Scenario: Nodes displayed as boxes with labels
- **WHEN** the TUI opens with a graph containing nodes
- **THEN** each node is displayed as a rectangular box with its label inside

#### Scenario: Edges displayed as directed lines
- **WHEN** the TUI opens with a graph containing edges
- **THEN** each edge is displayed as a line with an arrowhead pointing from source to target

#### Scenario: Edge labels shown when space permits
- **WHEN** an edge has a type label and there is sufficient space on the canvas
- **THEN** the edge type label is displayed along the edge line

#### Scenario: Nodes colored by type
- **WHEN** the TUI displays nodes
- **THEN** nodes of type "note" are shown in blue, "entity" in green, and "tag" in yellow

#### Scenario: Empty graph shows message
- **WHEN** the TUI opens with no nodes to display
- **THEN** the canvas shows a centered message "No nodes to display"

### Requirement: Node selection and details panel

The TUI SHALL allow selecting a node and viewing its details in a side panel.

#### Scenario: Select node with arrow keys
- **WHEN** user presses arrow keys
- **THEN** the selection highlight moves to the nearest node in that direction

#### Scenario: Select node with mouse click
- **WHEN** user clicks on a node with the mouse
- **THEN** that node becomes selected

#### Scenario: Details panel shows node information
- **WHEN** a node is selected
- **THEN** the details panel shows the node's label, type, and neighbor count

#### Scenario: Details panel shows neighbors
- **WHEN** a node is selected
- **THEN** the details panel lists its direct neighbors with edge type and weight

#### Scenario: Details panel shows metadata
- **WHEN** a selected node has metadata
- **THEN** the details panel shows the metadata content (truncated if long)

### Requirement: Keyboard navigation

The TUI SHALL support keyboard navigation with arrow keys, vim keys, and special keys.

#### Scenario: Arrow keys move selection
- **WHEN** user presses Up/Down/Left/Right arrow
- **THEN** selection moves to the nearest node in that direction

#### Scenario: Vim keys move selection
- **WHEN** user presses `h`/`j`/`k`/`l`
- **THEN** selection moves left/down/up/right respectively

#### Scenario: Enter selects node
- **WHEN** user presses Enter
- **THEN** the currently highlighted node becomes selected and its details are shown

#### Scenario: Q quits the TUI
- **WHEN** user presses `q` or Escape
- **THEN** the TUI exits cleanly

### Requirement: Filter by node type

The TUI SHALL allow filtering visible nodes by type.

#### Scenario: Cycle filter with `t` key
- **WHEN** user presses `t`
- **THEN** the filter cycles through: all → notes only → entities only → tags only → all

#### Scenario: Filtered nodes hidden from canvas
- **WHEN** a filter is active
- **THEN** nodes not matching the filter type are hidden from the canvas

#### Scenario: Filtered edges hidden
- **WHEN** a filter hides one or both endpoints of an edge
- **THEN** that edge is also hidden from the canvas

#### Scenario: Status bar shows active filter
- **WHEN** a filter is active
- **THEN** the status bar displays the current filter mode

### Requirement: Node search

The TUI SHALL support searching for nodes by name.

#### Scenario: `/` opens search mode
- **WHEN** user presses `/`
- **THEN** a search input field appears in the status bar

#### Scenario: Search highlights matching nodes
- **WHEN** user types a search query and presses Enter
- **THEN** nodes whose labels contain the query text are highlighted

#### Scenario: Escape cancels search
- **WHEN** user presses Escape during search input
- **THEN** search mode is cancelled and the search input is cleared

#### Scenario: Search is case-insensitive
- **WHEN** user types "python" in search
- **THEN** nodes labeled "Python", "PYTHON", "python3" etc. all match

### Requirement: Force-directed layout

The system SHALL position nodes using a force-directed layout algorithm for organic graph visualization.

#### Scenario: Nodes positioned without overlap
- **WHEN** the layout algorithm runs
- **THEN** no two nodes occupy the same position

#### Scenario: Connected nodes are closer
- **WHEN** the layout algorithm runs
- **THEN** nodes connected by edges are positioned closer together than unconnected nodes

#### Scenario: Layout fits within canvas
- **WHEN** the layout algorithm completes
- **THEN** all node positions are within the canvas boundaries

### Requirement: Status bar

The TUI SHALL display a status bar with contextual information.

#### Scenario: Status bar shows node count
- **WHEN** the TUI is running
- **THEN** the status bar shows the total number of visible nodes

#### Scenario: Status bar shows keybindings
- **WHEN** the TUI is running
- **THEN** the status bar shows available keybindings (arrows, q, t, /)

#### Scenario: Status bar shows selected node info
- **WHEN** a node is selected
- **THEN** the status bar shows the selected node's label and type
