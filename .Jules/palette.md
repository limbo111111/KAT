## 2024-05-12 - Added selection symbol to captures list table
**Learning:** In TUI applications using `ratatui` (like KAT), using only `Modifier::REVERSED` for table row selection can be insufficient for visibility, especially across different terminal color themes.
**Action:** Always consider supplementing style-based selection indicators with explicit text symbols (e.g., `.highlight_symbol(">> ")`) to ensure robust accessibility and visual clarity.
