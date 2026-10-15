-- The Blizzard Settings window (Esc > Options > AddOns). Settings.OpenToCategory
-- shows it with the addon category list on the left and the opened category
-- on the right: a canvas category's own frame is placed in the container,
-- a vertical-layout category gets rows of checkboxes, sliders and dropdowns
-- bound to its settings, like the client builds them.
local M = {}

local function build(sim)
  local env = sim.env
  local p = env.CreateFrame("Frame", nil, env.UIParent, "BasicFrameTemplateWithInset")
  p:SetSize(1000, 700)
  p:SetPoint("CENTER", 0, 20)
  p:SetFrameStrata("HIGH")
  p:SetToplevel(true)
  p.TitleText:SetText(env.SETTINGS or "Options")
  p.CloseButton:SetScript("OnClick", function() p:Hide() end)
  local list = env.CreateFrame("Frame", nil, p)
  list:SetPoint("TOPLEFT", 12, -34)
  list:SetPoint("BOTTOMLEFT", 12, 12)
  list:SetWidth(210)
  local header = list:CreateFontString(nil, "OVERLAY", "GameFontNormalLarge")
  header:SetPoint("TOPLEFT", 6, -4)
  header:SetText(env.ADDONS or "AddOns")
  p.List, p.rows = list, {}
  local container = env.CreateFrame("Frame", nil, p)
  container:SetPoint("TOPLEFT", list, "TOPRIGHT", 12, 0)
  container:SetPoint("BOTTOMRIGHT", -12, 12)
  p.Container = container
  return p
end

local function clearContainer(sim, p)
  for _, f in ipairs(p.generated or {}) do f:Hide() end
  p.generated = {}
  if p.canvas then p.canvas:Hide(); p.canvas = nil end
end

local function valueLabel(options, v)
  local data = type(options) == "function" and options() or options
  if type(data) == "table" and data.GetData then data = data:GetData() end
  for _, o in ipairs(type(data) == "table" and data or {}) do
    if o.value == v then return o.label or o.text or tostring(v) end
  end
  return tostring(v)
end

local function verticalLayout(sim, p, cat)
  local env = sim.env
  local c = p.Container
  local gen = p.generated
  local title = c:CreateFontString(nil, "OVERLAY", "GameFontHighlightHuge")
  title:SetPoint("TOPLEFT", 8, -4)
  title:SetText(cat.name)
  gen[#gen + 1] = title
  local y = -48
  for _, ctl in ipairs(cat.controls or {}) do
    local row = env.CreateFrame("Frame", nil, c)
    row:SetPoint("TOPLEFT", 0, y)
    row:SetPoint("RIGHT")
    row:SetHeight(28)
    gen[#gen + 1] = row
    local label = row:CreateFontString(nil, "OVERLAY", ctl.kind == "header" and "GameFontNormalLarge" or "GameFontNormal")
    label:SetPoint("LEFT", ctl.kind == "header" and 8 or 36, 0)
    label:SetText(ctl.kind == "header" and ctl.name or (ctl.setting and ctl.setting:GetName() or "?"))
    local s = ctl.setting
    if ctl.kind == "checkbox" then
      local cb = env.CreateFrame("CheckButton", nil, row, "UICheckButtonTemplate")
      cb:SetPoint("LEFT", 360, 0)
      cb:SetChecked(s:GetValue() and true or false)
      cb:SetScript("OnClick", function(b) s:SetValue(b:GetChecked() and true or false) end)
    elseif ctl.kind == "slider" then
      local sl = env.CreateFrame("Slider", nil, row, "OptionsSliderTemplate")
      sl:SetPoint("LEFT", 360, 0)
      sl:SetSize(200, 17)
      local o = ctl.options or {}
      sl:SetMinMaxValues(o.minValue or 0, o.maxValue or 1)
      sl:SetValueStep(o.steps or 1)
      sl:SetValue(tonumber(s:GetValue()) or o.minValue or 0)
      local v = sl:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
      v:SetPoint("LEFT", sl, "RIGHT", 10, 0)
      local val = s:GetValue()
      local fmt = o.formatter
      local ok, txt = true, tostring(val)
      if type(fmt) == "function" then ok, txt = pcall(fmt, val) end
      v:SetText(ok and tostring(txt) or tostring(val))
      sl:SetScript("OnValueChanged", function(_, nv) s:SetValue(nv); v:SetText(tostring(nv)) end)
    elseif ctl.kind == "dropdown" then
      local dd = env.CreateFrame("Button", nil, row, "UIPanelButtonTemplate")
      dd:SetPoint("LEFT", 360, 0)
      dd:SetSize(200, 24)
      dd:SetText(valueLabel(ctl.options, s:GetValue()))
    end
    y = y - (ctl.kind == "header" and 36 or 30)
  end
end

-- Show the window opened to `cat` (a category from Settings.Register*).
function M.open(sim, cat)
  local p = sim.settingsPanel
  if not p then p = build(sim); sim.settingsPanel = p end
  -- category list
  for _, r in ipairs(p.rows) do r:Hide() end
  local y = -30
  local function addRow(c, depth)
    local r = p.List:CreateFontString(nil, "OVERLAY", c == cat and "GameFontHighlight" or "GameFontNormal")
    r:SetPoint("TOPLEFT", 10 + depth * 14, y)
    r:SetText(c.name)
    if c == cat then r:SetTextColor(1, 1, 1) end
    p.rows[#p.rows + 1] = r
    y = y - 20
    for _, sub in ipairs(c.subcategories or {}) do addRow(sub, depth + 1) end
  end
  for _, c in ipairs(sim.settingsCategories) do
    if type(c) == "table" and c.name and not c.parent then addRow(c, 0) end
  end

  clearContainer(sim, p)
  if cat then
    if cat.frame then
      local f = cat.frame
      f:SetParent(p.Container)
      f:ClearAllPoints()
      f:SetAllPoints(p.Container)
      f:Show()
      p.canvas = f
    else
      verticalLayout(sim, p, cat)
    end
  end
  p:Show()
  return p
end

return M
