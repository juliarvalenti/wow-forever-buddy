# Game facts that shape what you propose

- **Approved settings reach the game at the next /reload or login.** The addon reads them at load.
- **New files need a restart.** A new addon, new art or a new font is only found when WoW starts; a /reload isn't enough.
- **The app writes game files only while WoW is closed.** If WoW is running, a change waits.
- **Positions belong to Edit Mode.** Where Blizzard's bars and frames sit is an Edit Mode layout. The player can import a layout string (Esc > Edit Mode > Import).
- **Combat locks the UI.** Nothing protected changes in combat; changes wait until combat ends.
- **Some values are hidden in combat.** On Forever, some numbers (health especially) can be secret to addons during combat. Designs must still work then: plain bars always do; percentages or arcs may fall back.
- **Combo points belong to the target** on Forever: they reset when the target changes.
- **Forever is the 12.x client API** with its own differences; the class list and abilities follow Forever, not retail.
