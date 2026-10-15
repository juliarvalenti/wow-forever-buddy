-- AnimationGroups and Animations (Alpha, Translation, Scale, Rotation,
-- VertexColor, TextureCoordTranslation, FlipBook, Path), driven by the
-- simulated clock in Sim:Advance.
local M = {}

local SMOOTH = {
  NONE = function(t) return t end,
  IN = function(t) return t * t end,
  OUT = function(t) return 1 - (1 - t) * (1 - t) end,
  IN_OUT = function(t) if t < 0.5 then return 2 * t * t end return 1 - 2 * (1 - t) * (1 - t) end,
}

local ANIM_TYPES = { Alpha = true, Translation = true, LineTranslation = true, Scale = true, LineScale = true,
  Rotation = true, VertexColor = true, TextureCoordTranslation = true, FlipBook = true, Path = true, Animation = true }

function M.install(sim, classes, define, state, S, create)
  sim.activeAnimGroups = sim.activeAnimGroups or {}

  local function fire(obj, script, ...) sim:_runScript(obj, script, ...) end

  ------------------------------------------------------------ creation
  local R = classes.Region.methods
  function R:CreateAnimationGroup(name, template)
    local g = create(sim, "AnimationGroup", name, nil)
    local gs = state[g]
    gs.parent = self
    gs.anims = {}
    gs.looping = "NONE"
    gs.playing = false
    local s = S(self)
    s.animGroups = s.animGroups or {}
    table.insert(s.animGroups, g)
    if template then require("wowapi.templates").apply(sim, g, template, create) end
    return g
  end
  function R:GetAnimationGroups()
    return unpack(S(self).animGroups or {})
  end
  function R:StopAnimating()
    for _, g in ipairs(S(self).animGroups or {}) do g:Stop() end
  end

  ------------------------------------------------------------ group
  local G = classes.AnimationGroup.methods
  function G:CreateAnimation(atype, name, template)
    atype = atype or "Animation"
    for t in pairs(ANIM_TYPES) do if t:lower() == tostring(atype):lower() then atype = t end end
    if not ANIM_TYPES[atype] then error("AnimationGroup:CreateAnimation(): Unknown animation type " .. tostring(atype), 2) end
    local a = create(sim, atype, name, nil)
    local as = state[a]
    as.parent = self
    as.duration, as.startDelay, as.endDelay, as.order = 0, 0, 0, 1
    as.smoothing = "NONE"
    table.insert(S(self).anims, a)
    if template then require("wowapi.templates").apply(sim, a, template, create) end
    return a
  end
  function G:GetAnimations() return unpack(S(self).anims) end
  function G:SetLooping(l) S(self).looping = l or "NONE" end
  function G:GetLooping() return S(self).looping end
  function G:GetLoopState() return S(self).loopState or "NONE" end
  function G:SetToFinalAlpha(v) S(self).toFinalAlpha = v and true or false end
  function G:IsSetToFinalAlpha() return S(self).toFinalAlpha or false end
  function G:IsPlaying() return S(self).playing and not S(self).paused end
  function G:IsPaused() return S(self).paused or false end
  function G:IsDone() return S(self).done or false end
  function G:IsPendingFinish() return false end
  function G:IsReverse() return S(self).reverse or false end
  function G:SetPlaying(v) if v then self:Play() else self:Stop() end end

  local function orders(g)
    local gs = state[g]
    local byOrder, list = {}, {}
    for _, a in ipairs(gs.anims) do
      local o = state[a].order or 1
      if not byOrder[o] then byOrder[o] = {}; list[#list + 1] = o end
      table.insert(byOrder[o], a)
    end
    table.sort(list)
    return list, byOrder
  end
  local function totalDuration(g)
    local list, byOrder = orders(g)
    local total = 0
    for _, o in ipairs(list) do
      local seg = 0
      for _, a in ipairs(byOrder[o]) do
        local as = state[a]
        seg = math.max(seg, as.startDelay + as.duration + as.endDelay)
      end
      total = total + seg
    end
    return total, list, byOrder
  end

  local function targetOf(a)
    local as = state[a]
    if as.target then return as.target end
    local g = as.parent
    local region = state[g].parent
    if as.childKey and region then return region[as.childKey] end
    if as.targetKey and region then
      local t = region
      for part in as.targetKey:gmatch("[^.]+") do
        if part == "$parent" then t = t:GetParent() else t = t and t[part] end
      end
      return t
    end
    return region
  end

  local function snapshot(a)
    local as = state[a]
    local t = targetOf(a)
    local ts = t and state[t]
    if not ts then return end
    as.baseAlpha = ts.alpha
    as.baseScale = ts.animScale or 1
    as.baseColor = ts.color
  end

  -- apply animation `a` at local progress p (0..1)
  local function apply(a, p)
    local as = state[a]
    local t = targetOf(a)
    local ts = t and state[t]
    if not ts then return end
    local e = (SMOOTH[as.smoothing] or SMOOTH.NONE)(p)
    as.progress = p
    local typ = as.type
    if typ == "Alpha" then
      local from = as.fromAlpha or ts.alpha
      local to = as.toAlpha or ts.alpha
      ts.alpha = from + (to - from) * e
    elseif typ == "Translation" or typ == "LineTranslation" then
      local ox, oy = as.offsetX or 0, as.offsetY or 0
      ts.animOffsets = ts.animOffsets or {}
      ts.animOffsets[a] = { ox * e, oy * e }
    elseif typ == "Scale" or typ == "LineScale" then
      local fx, fy = as.scaleFromX or 1, as.scaleFromY or 1
      local tx, ty = as.scaleToX or 1, as.scaleToY or 1
      ts.animScale = fx + (tx - fx) * e
      ts.animScaleY = fy + (ty - fy) * e
    elseif typ == "Rotation" then
      ts.animRotation = (as.degrees or 0) * e
    elseif typ == "VertexColor" then
      local c1, c2 = as.startColor, as.endColor
      if c1 and c2 then
        ts.color = { c1[1] + (c2[1] - c1[1]) * e, c1[2] + (c2[2] - c1[2]) * e, c1[3] + (c2[3] - c1[3]) * e,
          (c1[4] or 1) + ((c2[4] or 1) - (c1[4] or 1)) * e }
      end
    elseif typ == "FlipBook" then
      local frames = (as.flipRows or 1) * (as.flipCols or 1)
      as.flipFrame = math.min(frames - 1, math.floor(e * frames))
    end
    if as.scripts and as.scripts.OnUpdate then fire(a, "OnUpdate", 0) end
  end

  local function restore(g, final)
    for _, a in ipairs(state[g].anims) do
      local as = state[a]
      local t = targetOf(a)
      local ts = t and state[t]
      if ts then
        if ts.animOffsets then ts.animOffsets[a] = nil end
        if as.type == "Alpha" and as.baseAlpha and not (final and state[g].toFinalAlpha) then ts.alpha = as.baseAlpha end
        if as.type == "Scale" or as.type == "LineScale" then ts.animScale, ts.animScaleY = nil, nil end
        if as.type == "Rotation" then ts.animRotation = nil end
        if as.type == "VertexColor" and not final then ts.color = as.baseColor end
      end
      as.playing = false
    end
  end

  function G:Play(reverse, offset)
    local gs = S(self)
    if gs.playing and not gs.paused then return end
    if gs.paused then gs.paused = false; return end
    gs.playing, gs.done, gs.elapsed, gs.reverse = true, false, offset or 0, reverse and true or false
    gs.loopState = "NONE"
    for _, a in ipairs(gs.anims) do snapshot(a); state[a].playing = true; state[a].started = false end
    sim.activeAnimGroups[self] = true
    fire(self, "OnPlay")
  end
  function G:Restart(reverse, offset) self:Stop(); self:Play(reverse, offset) end
  function G:Pause() S(self).paused = true; fire(self, "OnPause") end
  function G:Stop()
    local gs = S(self)
    if not gs.playing then return end
    gs.playing, gs.paused = false, false
    sim.activeAnimGroups[self] = nil
    restore(self, false)
    fire(self, "OnStop", false)
  end
  function G:Finish()
    local gs = S(self)
    if not gs.playing then return end
    sim._tickAnimGroup(self, math.huge)
  end
  function G:GetDuration() return (totalDuration(self)) end
  function G:GetElapsed() return S(self).elapsed or 0 end
  function G:GetProgress()
    local total = totalDuration(self)
    return total > 0 and math.min(1, (S(self).elapsed or 0) / total) or 0
  end
  function G:SetAnimationSpeedMultiplier(m) S(self).speed = m end
  function G:GetAnimationSpeedMultiplier() return S(self).speed or 1 end
  function G:RemoveAnimations() S(self).anims = {} end

  -- advance one group by dt seconds
  local function tickGroup(g, dt)
    local gs = state[g]
    if not gs or not gs.playing or gs.paused then return end
    gs.elapsed = gs.elapsed + dt * (gs.speed or 1)
    local total, list, byOrder = totalDuration(g)
    local t = gs.elapsed
    local finished = t >= total
    if finished and gs.looping ~= "NONE" and dt ~= math.huge and total > 0 then
      local loops = math.floor(t / total)
      t = t - loops * total
      gs.elapsed = t
      if gs.looping == "BOUNCE" then
        gs.reverse = (loops % 2 == 1) ~= (gs.reverse or false)
      end
      gs.loopState = gs.reverse and "REVERSE" or "FORWARD"
      fire(g, "OnLoop", gs.loopState)
      finished = false
    end
    if finished then t = total end
    local pos = gs.reverse and (total - t) or t
    local segStart = 0
    for _, o in ipairs(list) do
      local seg = 0
      for _, a in ipairs(byOrder[o]) do
        local as = state[a]
        seg = math.max(seg, as.startDelay + as.duration + as.endDelay)
      end
      for _, a in ipairs(byOrder[o]) do
        local as = state[a]
        local local_ = pos - segStart - as.startDelay
        if local_ >= 0 then
          if not as.started then as.started = true; fire(a, "OnPlay") end
          local p = as.duration > 0 and math.min(1, local_ / as.duration) or 1
          apply(a, p)
          if p >= 1 and not as.finishedFired then as.finishedFired = true; fire(a, "OnFinished", false) end
        end
      end
      segStart = segStart + seg
    end
    if finished then
      gs.playing = false
      gs.done = true
      sim.activeAnimGroups[g] = nil
      restore(g, true)
      for _, a in ipairs(gs.anims) do state[a].finishedFired = false end
      fire(g, "OnFinished", false)
    end
  end

  sim._tickAnimGroup = tickGroup

  ------------------------------------------------------------ animation
  local A = classes.Animation.methods
  function A:SetDuration(d) S(self).duration = d end
  function A:GetDuration() return S(self).duration end
  function A:SetStartDelay(d) S(self).startDelay = d end
  function A:GetStartDelay() return S(self).startDelay end
  function A:SetEndDelay(d) S(self).endDelay = d end
  function A:GetEndDelay() return S(self).endDelay end
  function A:SetOrder(o) S(self).order = o end
  function A:GetOrder() return S(self).order end
  function A:SetSmoothing(s) S(self).smoothing = s end
  function A:GetSmoothing() return S(self).smoothing end
  function A:SetTarget(t) S(self).target = t end
  function A:GetTarget() return targetOf(self) end
  function A:SetTargetKey(k) S(self).targetKey = k end
  function A:SetChildKey(k) S(self).childKey = k end
  function A:SetTargetName(n) S(self).target = sim:Get(n) end
  function A:SetTargetParent() S(self).target = nil end
  function A:GetRegionParent() return state[S(self).parent].parent end
  function A:GetProgress() return S(self).progress or 0 end
  function A:GetSmoothProgress() return (SMOOTH[S(self).smoothing] or SMOOTH.NONE)(S(self).progress or 0) end
  function A:GetElapsed() return (S(self).progress or 0) * S(self).duration end
  function A:IsPlaying() return S(self).playing or false end
  function A:IsDone() return (S(self).progress or 0) >= 1 end
  function A:IsStopped() return not S(self).playing end
  function A:IsPaused() return false end
  function A:IsDelaying() return false end
  function A:Play() S(self).parent:Play() end
  function A:Stop() S(self).parent:Stop() end
  function A:Pause() S(self).parent:Pause() end

  local Al = classes.Alpha.methods
  function Al:SetFromAlpha(a) S(self).fromAlpha = a end
  function Al:GetFromAlpha() return S(self).fromAlpha or 0 end
  function Al:SetToAlpha(a) S(self).toAlpha = a end
  function Al:GetToAlpha() return S(self).toAlpha or 0 end

  local Tr = classes.Translation.methods
  function Tr:SetOffset(x, y) S(self).offsetX, S(self).offsetY = x, y end
  function Tr:GetOffset() return S(self).offsetX or 0, S(self).offsetY or 0 end

  local Sc = classes.Scale.methods
  function Sc:SetScale(x, y) S(self).scaleFromX, S(self).scaleFromY, S(self).scaleToX, S(self).scaleToY = 1, 1, x, y end
  function Sc:GetScale() return S(self).scaleToX or 1, S(self).scaleToY or 1 end
  function Sc:SetScaleFrom(x, y) S(self).scaleFromX, S(self).scaleFromY = x, y end
  function Sc:GetScaleFrom() return S(self).scaleFromX or 1, S(self).scaleFromY or 1 end
  function Sc:SetScaleTo(x, y) S(self).scaleToX, S(self).scaleToY = x, y end
  function Sc:GetScaleTo() return S(self).scaleToX or 1, S(self).scaleToY or 1 end
  function Sc:SetOrigin(p, x, y) S(self).origin = { p, x, y } end
  function Sc:GetOrigin() local o = S(self).origin or { "CENTER", 0, 0 }; return o[1], o[2], o[3] end

  local Ro = classes.Rotation.methods
  function Ro:SetDegrees(d) S(self).degrees = d end
  function Ro:GetDegrees() return S(self).degrees or 0 end
  function Ro:SetRadians(r) S(self).degrees = math.deg(r) end
  function Ro:GetRadians() return math.rad(S(self).degrees or 0) end
  function Ro:SetOrigin(p, x, y) S(self).origin = { p, x, y } end
  function Ro:GetOrigin() local o = S(self).origin or { "CENTER", 0, 0 }; return o[1], o[2], o[3] end

  local VC = classes.VertexColor.methods
  local function colorArgs(c, g, b, a)
    if type(c) == "table" then return { c.r, c.g, c.b, c.a or 1 } end
    return { c, g, b, a or 1 }
  end
  function VC:SetStartColor(...) S(self).startColor = colorArgs(...) end
  function VC:SetEndColor(...) S(self).endColor = colorArgs(...) end
  function VC:GetStartColor() local c = S(self).startColor or { 1, 1, 1, 1 }; return sim.env.CreateColor(c[1], c[2], c[3], c[4]) end
  function VC:GetEndColor() local c = S(self).endColor or { 1, 1, 1, 1 }; return sim.env.CreateColor(c[1], c[2], c[3], c[4]) end

  local FB = classes.FlipBook.methods
  function FB:SetFlipBookRows(n) S(self).flipRows = n end
  function FB:GetFlipBookRows() return S(self).flipRows or 1 end
  function FB:SetFlipBookColumns(n) S(self).flipCols = n end
  function FB:GetFlipBookColumns() return S(self).flipCols or 1 end
  function FB:SetFlipBookFrames(n) S(self).flipFrames = n end
  function FB:GetFlipBookFrames() return S(self).flipFrames or 0 end

  -- AnimationGroup/Animation are not regions: give them the script API.
  for _, cls in ipairs({ G, A }) do
    cls.SetScript = function(self, name, fn) S(self).scripts[name] = fn; S(self).hooks[name] = nil end
    cls.GetScript = function(self, name) return S(self).scripts[name] end
    cls.HookScript = function(self, name, fn)
      local s = S(self)
      if not s.scripts[name] then s.scripts[name] = fn; return end
      s.hooks[name] = s.hooks[name] or {}
      table.insert(s.hooks[name], fn)
    end
    cls.HasScript = function() return true end
    cls.GetParent = function(self) return S(self).parent end
  end
end

-- Called every frame by Sim:Advance.
function M.tick(sim, dt)
  local groups = {}
  for g in pairs(sim.activeAnimGroups) do groups[#groups + 1] = g end
  table.sort(groups, function(a, b) return sim.widgetState[a].seq < sim.widgetState[b].seq end)
  for _, g in ipairs(groups) do sim._tickAnimGroup(g, dt) end
end

return M
