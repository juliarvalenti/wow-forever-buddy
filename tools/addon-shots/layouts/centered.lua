-- The main bar centred at the bottom, as retail's Modern layout has it, with
-- the micro menu and bags moved to the bottom-right corner out of its way.
-- Each move is an Edit Mode system (Enum.EditModeSystem) and, for systems
-- with several frames, its index (Enum.EditMode<System>SystemIndices).
return {
    { system = "ActionBar", index = "MainBar", point = "BOTTOM", y = 45 },
    { system = "MicroMenu", point = "BOTTOMRIGHT", x = -4, y = 4 },
    { system = "Bags", point = "BOTTOMRIGHT", relativeTo = "MicroMenuContainer", relativePoint = "TOPRIGHT", y = 4 },
}
