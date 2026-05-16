## 2024-05-14 - Empty States in Terminal UIs
**Learning:** Adding empty states to list views in ratatui helps users understand system state, especially when background tasks (like receiving signals) are occurring.
**Action:** Always consider what a list component displays when it has no data, and provide context-aware messages to the user.
## 2024-05-16 - Avoid Blinking Text Modifiers
**Learning:** Using `Modifier::RAPID_BLINK` in ratatui TUIs is considered a severe accessibility anti-pattern (flashes/blinking text) that can trigger photosensitive issues and is highly inconsistent across terminals.
**Action:** Never use `Modifier::RAPID_BLINK` or similar blinking effects for cursors or active states. Rely on standard colors, bolding, or reversing styles for focus indicators instead.
