-- Julia's rogue layout (TH1): unit frames either side of the character,
-- the main bar right under them, the cast bar between; micro menu and bags
-- out of the way in the corner. The character stands a little below the
-- screen's centre, so the frames sit there too.
return {
    { system = "UnitFrame", index = "Player", point = "RIGHT", relativePoint = "CENTER", x = -150, y = -60 },
    { system = "UnitFrame", index = "Target", point = "LEFT", relativePoint = "CENTER", x = 150, y = -60 },
    { system = "ActionBar", index = "MainBar", point = "BOTTOM", y = 200 },
    { system = "ActionBar", index = "Bar2", point = "TOP", relativeTo = "MainActionBar", relativePoint = "BOTTOM", y = -22 },
    { system = "CastBar", point = "CENTER", y = -130 },
    { system = "MicroMenu", point = "BOTTOMRIGHT", x = -4, y = 4 },
    { system = "Bags", point = "BOTTOMRIGHT", relativeTo = "MicroMenuContainer", relativePoint = "TOPRIGHT", y = 4 },
}
