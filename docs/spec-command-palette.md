# Technical Specification: Command Palette (`Ctrl+P` / `Ctrl+Shift+P`)

## 1. Overview & Objective

To provide professional keyboard-centric interaction on par with developer-first tools (Zed, VS Code, Sublime Text, JetBrains), this specification defines the **Command Palette** for LogAnalysis.

The Command Palette allows engineers to access 100% of the application's capabilities—including file operations, filter management, real-time ADB streaming controls, view settings, and visual themes—without touching a mouse or memorizing complex multi-key combinations.

---

## 2. Palette Command Catalog

The initial catalog comprises core application commands organized into distinct domain categories:

| Category | Command Identifier | Display Title | Default Shortcut | Action Callback |
| :--- | :--- | :--- | :--- | :--- |
| **File** | `file:open` | File: Open Log File... | `Ctrl+O` | `workspace.prompt_open_file()` |
| **File** | `file:load_filters` | File: Load Filters Profile... | `Ctrl+Shift+O` | `workspace.prompt_load_filters()` |
| **File** | `file:save_filters` | File: Save Filters Profile | `Ctrl+S` | `workspace.save_filters_to_active_path()` |
| **File** | `file:save_filters_as` | File: Save Filters Profile As... | `Ctrl+Shift+S` | `workspace.prompt_save_filters()` |
| **Log** | `log:find` | Log: Find / Search in Log | `Ctrl+F` | `workspace.open_find()` |
| **Log** | `log:clear` | Log: Clear Loaded Logs | `Ctrl+K` | `workspace.clear_logs()` |
| **Filter** | `filter:toggle_filtered_only` | Filter: Toggle Show Only Filtered Lines | `Ctrl+H` | `workspace.toggle_filtered_only()` |
| **Filter** | `filter:add_rule` | Filter: Add New Filter Rule... | `Ctrl+Shift+F` | `workspace.open_filter_modal()` |
| **Filter** | `filter:add_tab` | Filter: Add New Filter Tab | `Ctrl+T` | `workspace.add_filter_tab()` |
| **Filter** | `filter:delete_tab` | Filter: Delete Current Filter Tab | `Ctrl+D` | `workspace.delete_current_filter_tab()` |
| **Filter** | `filter:rename_tab` | Filter: Rename Current Filter Tab | `Ctrl+R` | `workspace.rename_current_filter_tab()` |
| **ADB** | `adb:start_stream` | ADB: Start Live Logcat Stream | — | `workspace.start_adb_stream()` |
| **ADB** | `adb:stop_stream` | ADB: Stop Live Logcat Stream | — | `workspace.stop_adb_stream()` |
| **ADB** | `adb:toggle_pause` | ADB: Toggle Pause / Resume Stream | `Ctrl+Space` | `workspace.toggle_pause_stream()` |
| **ADB** | `adb:toggle_autoscroll` | ADB: Toggle Auto-Scroll to Bottom | — | `workspace.toggle_auto_scroll()` |
| **ADB** | `adb:rescan` | ADB: Rescan Connected Devices | — | `workspace.scan_adb_devices_nonblocking()` |
| **View** | `view:toggle_dock` | View: Toggle Filter Dock Position | `Ctrl+Shift+L` | `workspace.toggle_filter_panel_position()` |
| **View** | `view:toggle_full_line` | View: Toggle Full Line Wrap Mode | — | `workspace.toggle_full_line_display()` |
| **View** | `view:toggle_theme` | View: Toggle Dark / Light Theme | — | `workspace.toggle_theme()` |
| **View** | `view:zoom_in` | View: Zoom In (+) | `Ctrl+=` | `workspace.zoom(1.0)` |
| **View** | `view:zoom_out` | View: Zoom Out (-) | `Ctrl+-` | `workspace.zoom(-1.0)` |
| **View** | `view:zoom_reset` | View: Reset Zoom to Default | — | `workspace.font_size = px(13.0)` |

---

## 3. Fuzzy Scoring Algorithm

Given user query $Q$ and candidate title $T$:
1. If $Q$ is empty, all candidates match with score $0$ (preserving catalog order).
2. $Q$ must match $T$ as a case-insensitive subsequence. If any character in $Q$ does not appear in order in $T$, candidate is excluded.
3. **Scoring Function**:
   $$S(Q, T) = 100 \times \frac{|Q|}{|T|} + \sum_{i} \text{Bonus}(i)$$
   - **Prefix Match**: $+60$ points if match begins at index $0$.
   - **Word Boundary Match**: $+35$ points if character follows space `' '`, colon `':'`, or slash `'/'`.
   - **Consecutive Character Bonus**: $+15 \times (\text{consecutive\_count})$ points for uninterrupted matches.
   - **CamelCase Transition Bonus**: $+20$ points for uppercase letter directly following lowercase letter.

Candidates are ranked descending by $S(Q, T)$.

---

## 4. UI Architecture & Keyboard Navigation

### 4.1 Visual Hierarchy
- **Backdrop Overlay**: Semi-transparent dark blur (`rgba(0, 0, 0, 0.65)`), full window overlay with `occlude()`.
- **Palette Container**: Anchored top-center (`top: 72px`), width $580\text{px}$, max-height $460\text{px}$, rounded corners with subtle drop-shadow and $1\text{px}$ high-contrast border.
- **Search Header**:
  - Left icon: `>` or `🔍`
  - Input field: Native GPUI `Entity<TextInput>` with placeholder `"Type a command or search..."`
- **Results List**:
  - Virtualized/scrollable vertical list of matching commands.
  - Category pill badge (e.g., `FILE`, `ADB`, `VIEW`) on left.
  - Command title with highlighted fuzzy-matched characters.
  - Shortcut badge (e.g., `Ctrl+P`, `Ctrl+O`) right-aligned.
  - Active selection indicator with vibrant accent background (`rgba(theme.accent, 0.25)`).

### 4.2 Focus & Input Routing
1. **Open / Toggle Action**:
   - `Ctrl+P`, `Ctrl+Shift+P`, `Cmd+P`, `Cmd+Shift+P`.
   - When opened: sets `is_command_palette_open = true`, focuses `palette_input`, clears search query, resets selection index to 0.
2. **Keyboard Handlers**:
   - `ArrowDown`: Moves active selection index down (clamped to `results.len() - 1`).
   - `ArrowUp`: Moves active selection index up (clamped to 0).
   - `Enter`: Executes the highlighted command immediately and closes the palette.
   - `Escape`: Closes the palette and restores prior viewport focus.
   - `Click`: Clicking any command executes it and dismisses the palette; clicking the backdrop dismisses without action.

---

## 5. Verification & Test Plan

1. **Fuzzy Matcher Unit Tests**:
   - Exact match, prefix match, acronym/subsequence match (e.g., `"fdo"` matches `"File: Open Log File..."`).
   - Scoring precedence (exact prefix ranks higher than scattered characters).
   - Empty query returns full catalog.
2. **Command Dispatch Verification**:
   - Ensure every command in the catalog dispatches to its corresponding action callback safely.
3. **Workspace Integration**:
   - Clean compilation, zero UI freeze, 100% pass on deterministic test suite.
