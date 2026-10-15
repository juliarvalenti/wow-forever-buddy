-- Frame templates: the built-in Blizzard templates addons commonly
-- inherit, plus virtual templates defined in addon XML (see xml.lua).
local M = {}

local function trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end

function M.split(templates)
  local out = {}
  for t in tostring(templates or ""):gmatch("[^,]+") do
    t = trim(t)
    if t ~= "" then out[#out + 1] = t end
  end
  return out
end

local function child(sim, create, t, obj, suffix, key, layer)
  local s = sim.widgetState[obj]
  local name = s.name and (s.name .. suffix) or nil
  local c = create(sim, t, name, obj)
  if layer then sim.widgetState[c].layer = layer end
  obj[key] = c
  return c
end

-- Built-in templates: name -> function(sim, obj, create)
function M.builtins(env)
  local B = {}

  B.BackdropTemplate = function(sim, obj)
    for k, v in pairs(env.BackdropTemplateMixin) do obj[k] = v end
  end
  B.TooltipBackdropTemplate = function(sim, obj)
    B.BackdropTemplate(sim, obj)
    obj:SetBackdrop(env.BACKDROP_TOOLTIP_16_16_5555)
  end

  local function buttonText(sim, obj, create)
    local fs = child(sim, create, "FontString", obj, "Text", "Text", "OVERLAY")
    fs:SetFontObject("GameFontNormal")
    fs:SetPoint("CENTER")
    sim.widgetState[obj].fontString = fs
    sim.widgetState[obj].chrome = "panel"
  end
  for _, n in ipairs({ "UIPanelButtonTemplate", "UIPanelButtonNoTooltipTemplate", "UIMenuButtonStretchTemplate",
    "UIGoldBorderButtonTemplate", "MagicButtonTemplate", "SharedButtonSmallTemplate", "SharedButtonTemplate",
    "UIPanelDynamicResizeButtonTemplate", "TabButtonTemplate", "PanelTabButtonTemplate", "UIPanelSquareButton" }) do
    B[n] = buttonText
  end
  for _, n in ipairs({ "UIPanelCloseButton", "UIPanelCloseButtonNoScripts", "UIPanelHideButtonNoScripts" }) do
    B[n] = function(sim, obj)
      sim.widgetState[obj].width, sim.widgetState[obj].height = 24, 24
      sim.widgetState[obj].chrome = "close"
      obj:SetNormalAtlas("RedButton-Exit")
      obj:SetPushedAtlas("RedButton-exit-pressed")
      obj:SetDisabledAtlas("RedButton-Exit-Disabled")
      obj:SetHighlightAtlas("RedButton-Highlight")
      if n == "UIPanelCloseButton" then
        obj:SetScript("OnClick", function(self) local p = self:GetParent(); if p then p:Hide() end end)
      end
    end
  end

  local function checkText(sim, obj, create)
    local s = sim.widgetState[obj]
    s.width, s.height = 26, 26
    local fs = child(sim, create, "FontString", obj, "Text", "Text", "ARTWORK")
    fs:SetFontObject("GameFontHighlight")
    fs:SetPoint("LEFT", obj, "RIGHT", 2, 1)
    obj.text = fs
  end
  for _, n in ipairs({ "UICheckButtonTemplate", "InterfaceOptionsCheckButtonTemplate", "ChatConfigCheckButtonTemplate",
    "SettingsCheckboxTemplate", "UICheckButtonArtTemplate" }) do
    B[n] = checkText
  end

  -- a row in an options category tree (AceGUI TreeGroup, legacy options)
  B.OptionsListButtonTemplate = function(sim, obj, create)
    local s = sim.widgetState[obj]
    s.width, s.height = 175, 18
    local fs = child(sim, create, "FontString", obj, "Text", "text", "ARTWORK")
    fs:SetFontObject("GameFontNormal")
    fs:SetJustifyH("LEFT")
    fs:SetPoint("LEFT", 6, 0)
    fs:SetPoint("RIGHT", -20, 0)
    s.fontString = fs
    local toggle = child(sim, create, "Button", obj, "Toggle", "toggle")
    toggle:SetSize(14, 14)
    toggle:SetPoint("TOPRIGHT", -6, -1)
    toggle:SetNormalTexture("Interface\\Buttons\\UI-PlusButton-UP")
    toggle:SetPushedTexture("Interface\\Buttons\\UI-PlusButton-DOWN")
    toggle:SetHighlightTexture("Interface\\Buttons\\UI-PlusButton-Hilight")
    obj:SetHighlightTexture("Interface\\QuestFrame\\UI-QuestLogTitleHighlight")
  end

  local function slider(sim, obj, create)
    local s = sim.widgetState[obj]
    s.width, s.height = 144, 17
    s.orientation = "HORIZONTAL"
    child(sim, create, "FontString", obj, "Text", "Text", "ARTWORK"):SetPoint("BOTTOM", obj, "TOP")
    child(sim, create, "FontString", obj, "Low", "Low", "ARTWORK"):SetPoint("TOPLEFT", obj, "BOTTOMLEFT", -4, 3)
    child(sim, create, "FontString", obj, "High", "High", "ARTWORK"):SetPoint("TOPRIGHT", obj, "BOTTOMRIGHT", 4, 3)
    obj.Text:SetFontObject("GameFontNormalSmall")
    obj.Low:SetFontObject("GameFontHighlightSmall")
    obj.High:SetFontObject("GameFontHighlightSmall")
  end
  B.OptionsSliderTemplate = slider
  B.UISliderTemplate = slider
  B.UISliderTemplateWithLabels = slider
  B.MinimalSliderTemplate = function(sim, obj) sim.widgetState[obj].height = 17 end

  B.InputBoxTemplate = function(sim, obj)
    local s = sim.widgetState[obj]
    s.fontObject = "ChatFontNormal"
    obj:SetScript("OnEscapePressed", function(self) self:ClearFocus() end)
    obj:SetScript("OnEditFocusLost", function(self) self:HighlightText(0, 0) end)
  end
  B.InputBoxInstructionsTemplate = function(sim, obj, create)
    B.InputBoxTemplate(sim, obj)
    child(sim, create, "FontString", obj, "Instructions", "Instructions", "ARTWORK")
  end
  B.SearchBoxTemplate = B.InputBoxInstructionsTemplate

  local function scroll(sim, obj, create)
    local bar = child(sim, create, "Slider", obj, "ScrollBar", "ScrollBar")
    local up = child(sim, create, "Button", bar, "ScrollUpButton", "ScrollUpButton")
    up:SetSize(18, 16); up:SetPoint("BOTTOM", bar, "TOP")
    up:SetScript("OnClick", function() bar:SetValue(bar:GetValue() - 20) end)
    local down = child(sim, create, "Button", bar, "ScrollDownButton", "ScrollDownButton")
    down:SetSize(18, 16); down:SetPoint("TOP", bar, "BOTTOM")
    down:SetScript("OnClick", function() bar:SetValue(bar:GetValue() + 20) end)
    for _, b in ipairs({ up, down }) do
      local dir = b == up and "Up" or "Down"
      b:SetNormalTexture("Interface\\Buttons\\UI-ScrollBar-Scroll" .. dir .. "Button-Up")
      b:SetPushedTexture("Interface\\Buttons\\UI-ScrollBar-Scroll" .. dir .. "Button-Down")
      b:SetDisabledTexture("Interface\\Buttons\\UI-ScrollBar-Scroll" .. dir .. "Button-Disabled")
      b:SetHighlightTexture("Interface\\Buttons\\UI-ScrollBar-Scroll" .. dir .. "Button-Highlight")
      b.Normal, b.Pushed, b.Disabled, b.Highlight = b:GetNormalTexture(), b:GetPushedTexture(), b:GetDisabledTexture(), b:GetHighlightTexture()
    end
    bar.ThumbTexture = create(sim, "Texture", nil, bar)
    bar:SetThumbTexture(bar.ThumbTexture)
    bar:SetPoint("TOPLEFT", obj, "TOPRIGHT", 6, -16)
    bar:SetPoint("BOTTOMLEFT", obj, "BOTTOMRIGHT", 6, 16)
    bar:SetScript("OnValueChanged", function(self, v) obj:SetVerticalScroll(v) end)
    obj:EnableMouseWheel(true)
    obj:SetScript("OnMouseWheel", function(self, delta)
      local lo, hi = bar:GetMinMaxValues()
      bar:SetValue(math.max(lo, math.min(hi, bar:GetValue() - delta * 20)))
    end)
    obj:SetScript("OnScrollRangeChanged", function(self, x, y)
      bar:SetMinMaxValues(0, y or 0)
    end)
  end
  -- a standalone legacy scroll bar (Slider with arrow buttons)
  local function scrollBar(sim, obj, create)
    local s = sim.widgetState[obj]
    s.width = s.width ~= 0 and s.width or 16
    local up = child(sim, create, "Button", obj, "ScrollUpButton", "ScrollUpButton")
    up:SetSize(18, 16); up:SetPoint("BOTTOM", obj, "TOP")
    up:SetScript("OnClick", function() obj:SetValue(obj:GetValue() - 20) end)
    local down = child(sim, create, "Button", obj, "ScrollDownButton", "ScrollDownButton")
    down:SetSize(18, 16); down:SetPoint("TOP", obj, "BOTTOM")
    down:SetScript("OnClick", function() obj:SetValue(obj:GetValue() + 20) end)
    for _, b in ipairs({ up, down }) do
      local dir = b == up and "Up" or "Down"
      b:SetNormalTexture("Interface\\Buttons\\UI-ScrollBar-Scroll" .. dir .. "Button-Up")
      b:SetPushedTexture("Interface\\Buttons\\UI-ScrollBar-Scroll" .. dir .. "Button-Down")
      b:SetDisabledTexture("Interface\\Buttons\\UI-ScrollBar-Scroll" .. dir .. "Button-Disabled")
      b:SetHighlightTexture("Interface\\Buttons\\UI-ScrollBar-Scroll" .. dir .. "Button-Highlight")
      b.Normal, b.Pushed, b.Disabled, b.Highlight = b:GetNormalTexture(), b:GetPushedTexture(), b:GetDisabledTexture(), b:GetHighlightTexture()
    end
    obj.ThumbTexture = create(sim, "Texture", s.name and (s.name .. "ThumbTexture"), obj)
    obj.ThumbTexture:SetTexture("Interface\\Buttons\\UI-ScrollBar-Knob")
    obj:SetThumbTexture(obj.ThumbTexture)
  end
  B.UIPanelScrollBarTemplate = scrollBar
  B.UIPanelScrollBarTemplateLightBorder = scrollBar
  B.UIPanelStretchableArtScrollBarTemplate = scrollBar
  B.MinimalScrollBarTemplate = scrollBar
  B.UIDropDownMenuTemplate = function(sim, obj, create)
    local s = sim.widgetState[obj]
    s.width, s.height = s.width ~= 0 and s.width or 40, 32
    local function tex(suffix, key, file)
      local t = child(sim, create, "Texture", obj, suffix, key, "ARTWORK")
      t:SetTexture("Interface\\Glues\\CharacterCreate\\CharacterCreate-LabelFrame")
      return t
    end
    local l, m, r = tex("Left", "Left"), tex("Middle", "Middle"), tex("Right", "Right")
    l:SetSize(25, 64); l:SetPoint("TOPLEFT", 0, 17); l:SetTexCoord(0, 0.1953125, 0, 1)
    m:SetSize(115, 64); m:SetPoint("LEFT", l, "RIGHT"); m:SetTexCoord(0.1953125, 0.8046875, 0, 1)
    r:SetSize(25, 64); r:SetPoint("LEFT", m, "RIGHT"); r:SetTexCoord(0.8046875, 1, 0, 1)
    local text = child(sim, create, "FontString", obj, "Text", "Text", "ARTWORK")
    text:SetFontObject("GameFontHighlightSmall"); text:SetJustifyH("RIGHT"); text:SetSize(0, 10)
    text:SetPoint("RIGHT", r, "RIGHT", -43, 2)
    local icon = child(sim, create, "Texture", obj, "Icon", "Icon", "OVERLAY")
    icon:SetSize(16, 16); icon:SetPoint("LEFT", 30, 2); icon:Hide()
    local b = child(sim, create, "Button", obj, "Button", "Button")
    b:SetSize(24, 24); b:SetPoint("TOPRIGHT", r, "TOPRIGHT", -16, -18)
    b:SetNormalTexture("Interface\\ChatFrame\\UI-ChatIcon-ScrollDown-Up")
  end
  B.UIPanelScrollFrameTemplate = scroll
  B.ScrollFrameTemplate = scroll
  B.UIPanelScrollFrameCodeTemplate = scroll
  B.InputScrollFrameTemplate = function(sim, obj, create)
    scroll(sim, obj, create)
    obj.EditBox = create(sim, "EditBox", nil, obj)
    obj:SetScrollChild(obj.EditBox)
  end

  local function panel(sim, obj, create, title, inset)
    B.BackdropTemplate(sim, obj)
    obj:SetBackdrop(env.BACKDROP_DIALOG_32_32)
    local t = child(sim, create, "FontString", obj, title, title, "OVERLAY")
    t:SetFontObject("GameFontHighlight")
    t:SetPoint("TOP", 0, -5)
    local close = child(sim, create, "Button", obj, "CloseButton", "CloseButton")
    B.UIPanelCloseButton(sim, close)
    close:SetPoint("TOPRIGHT", 2, 1)
    if inset then
      local i = child(sim, create, "Frame", obj, "Inset", "Inset")
      i:SetPoint("TOPLEFT", 4, -24)
      i:SetPoint("BOTTOMRIGHT", -4, 4)
    end
  end
  B.BasicFrameTemplate = function(sim, obj, create) panel(sim, obj, create, "TitleText") end
  B.BasicFrameTemplateWithInset = function(sim, obj, create) panel(sim, obj, create, "TitleText", true) end
  B.PortraitFrameTemplate = function(sim, obj, create)
    panel(sim, obj, create, "TitleContainer")
    obj.TitleText = obj.TitleContainer
    obj.SetTitle = function(self, text) self.TitleText:SetText(text) end
    obj.GetTitleText = function(self) return self.TitleText end
    obj.PortraitContainer = create(sim, "Frame", nil, obj)
    obj.PortraitContainer.portrait = create(sim, "Texture", nil, obj.PortraitContainer)
    obj.SetPortraitToAsset = function(self, tex) self.PortraitContainer.portrait:SetTexture(tex) end
  end
  B.ButtonFrameTemplate = function(sim, obj, create)
    B.PortraitFrameTemplate(sim, obj, create)
    local i = child(sim, create, "Frame", obj, "Inset", "Inset")
    i:SetPoint("TOPLEFT", 4, -60)
    i:SetPoint("BOTTOMRIGHT", -6, 26)
  end
  B.DefaultPanelTemplate = B.BasicFrameTemplate
  B.DefaultPanelFlatTemplate = B.BasicFrameTemplate
  B.InsetFrameTemplate = function(sim, obj, create)
    local bg = child(sim, create, "Texture", obj, "Bg", "Bg", "BACKGROUND")
    bg:SetAtlas("UI-Frame-InnerBotTile")
    bg:SetPoint("TOPLEFT", 2, -2)
    bg:SetPoint("BOTTOMRIGHT", -2, 2)
    bg:SetVertexColor(0.3, 0.3, 0.3)
    obj.NineSlice = create(sim, "Frame", nil, obj)
    obj.NineSlice:SetAllPoints()
  end
  B.InsetFrameTemplate2 = B.InsetFrameTemplate
  B.InsetFrameTemplate3 = B.InsetFrameTemplate
  B.InsetFrameTemplate4 = B.InsetFrameTemplate
  B.TooltipBorderedFrameTemplate = B.TooltipBackdropTemplate
  B.GameTooltipTemplate = function(sim, obj) end
  B.SharedTooltipTemplate = B.GameTooltipTemplate

  -- Secure templates: frames become protected (combat lockdown applies).
  local sec = require("wowapi.secure")
  local function secure(sim, obj)
    sim.widgetState[obj].protected = true
  end
  -- SecureHandler* templates get the handler methods from SecureHandler_OnLoad.
  local function handler(sim, obj)
    secure(sim, obj)
    obj.SetFrameRef = function(self, label, ref) self:SetAttribute("frameref-" .. label, ref) end
    obj.Execute = function(self, body) sec.runSnippet(sim, self, body, "self") end
    obj.WrapScript = function(self, frame, script, pre, post) env.SecureHandlerWrapScript(frame, script, self, pre, post) end
    obj.UnwrapScript = function(self, frame, script) end
  end
  local function secureAction(sim, obj)
    secure(sim, obj)
    obj:RegisterForClicks("LeftButtonUp")
    obj:SetScript("OnClick", function(self, button) sec.performAction(sim, self, button) end)
  end
  local function handlerClick(sim, obj)
    handler(sim, obj)
    obj:SetScript("OnClick", function(self, button, down)
      local body = self:GetAttribute("_onclick")
      if body then sec.runSnippet(sim, self, body, "self, button, down", button, down) end
    end)
  end
  local function handlerShowHide(sim, obj)
    handler(sim, obj)
    obj:SetScript("OnShow", function(self) local b = self:GetAttribute("_onshow"); if b then sec.runSnippet(sim, self, b, "self") end end)
    obj:SetScript("OnHide", function(self) local b = self:GetAttribute("_onhide"); if b then sec.runSnippet(sim, self, b, "self") end end)
  end
  local function handlerEnterLeave(sim, obj)
    handler(sim, obj)
    obj:SetScript("OnEnter", function(self) local b = self:GetAttribute("_onenter"); if b then sec.runSnippet(sim, self, b, "self") end end)
    obj:SetScript("OnLeave", function(self) local b = self:GetAttribute("_onleave"); if b then sec.runSnippet(sim, self, b, "self") end end)
  end
  for _, n in ipairs({ "SecureFrameTemplate", "SecureActionButtonTemplate", "SecureUnitButtonTemplate",
    "SecureHandlerBaseTemplate", "SecureHandlerStateTemplate", "SecureHandlerClickTemplate",
    "SecureHandlerAttributeTemplate", "SecureHandlerShowHideTemplate", "SecureHandlerEnterLeaveTemplate",
    "SecureGroupHeaderTemplate", "SecurePartyHeaderTemplate", "SecureRaidGroupHeaderTemplate",
    "SecureHandlerDragTemplate", "SecureHandlerMouseUpDownTemplate", "SecureHandlerMouseWheelTemplate",
    "SecureAuraHeaderTemplate", "ActionButtonTemplate", "SecureActionButtonTemplateNoClick" }) do
    B[n] = n:find("^SecureHandler") and handler or secure
  end
  B.SecureActionButtonTemplate = secureAction
  B.SecureUnitButtonTemplate = function(sim, obj)
    secureAction(sim, obj)
    if not obj:GetAttribute("type1") then sim.widgetState[obj].attributes["type1"] = "target" end
  end
  B.ActionButtonTemplate = secureAction
  B.SecureHandlerClickTemplate = handlerClick
  B.SecureHandlerShowHideTemplate = handlerShowHide
  B.SecureHandlerEnterLeaveTemplate = handlerEnterLeave
  return B
end

function M.install(sim, env)
  sim.templates = {}
  for name, fn in pairs(M.builtins(env)) do sim.templates[name] = { builtin = fn } end
end

-- Apply a comma-separated template list to a freshly created widget.
function M.apply(sim, obj, templates, create, ctx)
  local own = ctx == nil
  if own then ctx = sim.xml.newCtx(sim, "CreateFrame") end
  for _, name in ipairs(M.split(templates)) do
    local t = sim.templates[name]
    if not t then
      error('CreateFrame(): Couldn\'t find inherited node "' .. name .. '"', 4)
    end
    if t.builtin then t.builtin(sim, obj, create)
    elseif t.xml then sim.xml.applyTemplate(sim, obj, t, ctx) end
  end
  if own then
    -- CreateFrame(..., template): anchors resolve and OnLoad fires right away
    sim.xml.applyAnchors(ctx)
    local s = sim.widgetState[obj]
    if s and s.scripts and s.scripts.OnLoad then table.insert(ctx.onloads, obj) end
    sim.xml.runOnLoads(sim, ctx)
  end
end

return M
