-- RAVET Delay host check. Generated and launched by tools/reaper-validation/delay.py
-- inside a disposable REAPER resource directory; do not run it in a normal session.
-- The driver prepends a RAVET_DELAY table (work directory, format prefix, rates,
-- delay cases). This part uses only public ReaScript API calls and runs
-- synchronously: discovery, parameter edits, renders, save/reopen recall, quit.
local C = assert(RAVET_DELAY, 'RAVET_DELAY configuration missing; run through delay.py')
local root = C.root
local log = assert(io.open(root .. '/delay-host.log', 'w'))
local function canonical(path)
  path = path:gsub('\\', '/'):gsub('/+$', '')
  if reaper.GetOS():find('Win', 1, true) then path = path:lower() end
  return path
end
if canonical(reaper.GetResourcePath()) ~= canonical(C.resource) then
  -- Wrong configuration (for example the normal user one): change nothing and
  -- do not quit, so REAPER saves no settings; the driver kills the process.
  log:write('ABORT resource directory is ' .. reaper.GetResourcePath() .. ', expected ' .. C.resource .. '\n')
  log:close()
  return
end
local function note(s) log:write(s .. '\n'); log:flush() end
local function check(ok, s) if not ok then error('check failed: ' .. s, 2) end; note('PASS ' .. s) end
local function near(a, b, tolerance) return a ~= nil and b ~= nil and math.abs(a - b) <= (tolerance or 1e-4) end

-- Physical declaration of each RAVET Delay control used here (examples/delay-plugin/src/lib.rs).
local SPEC = {
  time = {name = 'Time', min = 1, max = 2000, scale = 'log'},
  feedback = {name = 'Feedback', min = 0, max = 95},
  mix = {name = 'Mix', min = 0, max = 100},
  ping_pong = {name = 'Ping-pong', min = 0, max = 1, steps = 1},
  sync = {name = 'Sync', min = 0, max = 1, steps = 1},
  division = {name = 'Division', min = 0, max = 11, steps = 11},
}
local ORDER = {'time', 'feedback', 'mix', 'ping_pong', 'sync', 'division'}

local track, fx, index, physical

local function normalized_of(spec, value)
  if spec.scale == 'log' then return math.log(value / spec.min) / math.log(spec.max / spec.min) end
  return (value - spec.min) / (spec.max - spec.min)
end

local function formatted(i)
  local _, text = reaper.TrackFX_GetFormattedParamValue(track, fx, i, '')
  return text
end

-- Locate the plug-in's own controls by name. REAPER appends its wrapper
-- Bypass/Wet/Delta controls after the plug-in's, so the first match is used.
-- Each control is driven in physical units when REAPER reports the physical
-- range (CLAP, LV2) and in normalized units otherwise (VST3, AU).
local function locate()
  index, physical = {}, {}
  local count = reaper.TrackFX_GetNumParams(track, fx)
  for i = 0, count - 1 do
    local _, name = reaper.TrackFX_GetParamName(track, fx, i, '')
    local _, ident = reaper.TrackFX_GetParamIdent(track, fx, i)
    local _, minimum, maximum = reaper.TrackFX_GetParamEx(track, fx, i)
    note(string.format('Parameter %d name=%s ident=%s range=%g..%g', i, name, tostring(ident), minimum, maximum))
    for _, key in ipairs(ORDER) do
      local spec = SPEC[key]
      if index[key] == nil and name == spec.name then
        index[key] = i
        physical[key] = near(minimum, spec.min, 1e-3) and near(maximum, spec.max, 1e-3)
      end
    end
  end
  for _, key in ipairs(ORDER) do
    check(index[key] ~= nil, 'plug-in exposes parameter ' .. SPEC[key].name)
  end
end

local function physical_value(key)
  local spec, i = SPEC[key], index[key]
  if physical[key] then return reaper.TrackFX_GetParam(track, fx, i) end
  local n = reaper.TrackFX_GetParamNormalized(track, fx, i)
  if spec.steps then return spec.min + math.floor(n * spec.steps + 0.5) * (spec.max - spec.min) / spec.steps end
  local number = tonumber((formatted(i) or ''):match('-?%d+%.?%d*'))
  return number
end

local function set(key, value)
  local spec, i = SPEC[key], index[key]
  if physical[key] then
    reaper.TrackFX_SetParam(track, fx, i, value)
  elseif spec.steps then
    reaper.TrackFX_SetParamNormalized(track, fx, i, (value - spec.min) / (spec.max - spec.min))
  else
    -- Normalized continuous control: start from the declared mapping, then
    -- refine by bisection on REAPER's formatted readback if the format maps
    -- the range differently.
    local n = normalized_of(spec, value)
    reaper.TrackFX_SetParamNormalized(track, fx, i, n)
    local current = physical_value(key)
    if current == nil or not near(current, value, math.max(1e-3, math.abs(value) * 1e-3)) then
      local low, high = 0, 1
      for _ = 1, 40 do
        n = (low + high) / 2
        reaper.TrackFX_SetParamNormalized(track, fx, i, n)
        current = physical_value(key)
        if current == nil then break end
        if current < value then low = n else high = n end
      end
    end
  end
end

-- Wait (yielding to REAPER's main loop) until a condition holds or a timeout.
-- Host edits reach a CLAP plug-in only at its next process or flush call.
local function settle(condition, seconds)
  local deadline = reaper.time_precise() + (seconds or 10)
  while not condition() do
    if reaper.time_precise() > deadline then return false end
    coroutine.yield()
  end
  return true
end

local function expect(key, value, label)
  local spec = SPEC[key]
  local tolerance = spec.steps and 1e-6 or math.max(1e-3, math.abs(value) * 1e-3)
  local started = reaper.time_precise()
  settle(function() return near(physical_value(key), value, tolerance) end)
  local actual = physical_value(key)
  note(string.format('Readback %s=%s (%s) expected %g after %.2f s', key, tostring(actual),
    formatted(index[key]), value, reaper.time_precise() - started))
  check(near(actual, value, tolerance), label .. ': ' .. spec.name .. ' = ' .. value)
end

local function apply(values, label)
  for _, key in ipairs(ORDER) do if values[key] ~= nil then set(key, values[key]) end end
  for _, key in ipairs(ORDER) do if values[key] ~= nil then expect(key, values[key], label) end end
end

local function find_fx()
  track = reaper.GetTrack(0, 0)
  check(track ~= nil, 'project track present')
  fx = reaper.TrackFX_GetByName(track, C.fx_name, false)
  if fx < 0 and reaper.TrackFX_GetCount(track) == 1 then fx = 0 end
  check(fx >= 0, 'delay instance present on the track')
  locate()
end

local function render(case)
  reaper.GetSetProjectInfo(0, 'RENDER_SETTINGS', 0, true) -- master mix
  reaper.GetSetProjectInfo(0, 'RENDER_BOUNDSFLAG', 0, true) -- custom time bounds
  reaper.GetSetProjectInfo(0, 'RENDER_STARTPOS', 0, true)
  reaper.GetSetProjectInfo(0, 'RENDER_ENDPOS', C.length, true)
  reaper.GetSetProjectInfo(0, 'RENDER_CHANNELS', 2, true)
  reaper.GetSetProjectInfo(0, 'RENDER_SRATE', C.rate, true)
  reaper.GetSetProjectInfo(0, 'RENDER_TAILFLAG', 0, true)
  reaper.GetSetProjectInfo(0, 'RENDER_DITHER', 0, true)
  reaper.GetSetProjectInfo_String(0, 'RENDER_FORMAT', 'evaw', true)
  reaper.GetSetProjectInfo_String(0, 'RENDER_FILE', root, true)
  reaper.GetSetProjectInfo_String(0, 'RENDER_PATTERN', case.name, true)
  -- File: Render project, using the most recent render settings, auto-close render dialog.
  reaper.Main_OnCommand(42230, 0)
  local path = root .. '/' .. case.name .. '.wav'
  check(reaper.file_exists(path), 'rendered ' .. case.name .. '.wav')
  note('rendered ' .. case.name .. '=' .. path)
end

local function save(name)
  reaper.Main_SaveProjectEx(0, root .. '/' .. name .. '.rpp', 0)
  check(reaper.file_exists(root .. '/' .. name .. '.rpp'), 'saved project ' .. name .. '.rpp')
end

local function reopen(name)
  reaper.Main_openProject('noprompt:' .. root .. '/' .. name .. '.rpp')
  find_fx()
end

local function values_of(case)
  return {time = case.time_ms, feedback = 0, mix = case.mix, ping_pong = 0,
          sync = case.sync and 1 or 0, division = case.division}
end

local function run()
  note('Resource ' .. reaper.GetResourcePath())
  note('REAPER ' .. reaper.GetAppVersion())
  note('Platform ' .. reaper.GetOS())
  note('Format ' .. C.format .. ' requested ' .. C.fx_name)
  check(reaper.CountTracks(0) == 0, 'fresh empty project')

  local candidates = {}
  for i = 0, 100000 do
    local ok, name = reaper.EnumInstalledFX(i)
    if not ok then break end
    if name:find('RAVET Delay', 1, true) then candidates[#candidates + 1] = name end
  end
  note('Installed RAVET Delay candidates: ' .. (#candidates > 0 and table.concat(candidates, ' | ') or 'none'))
  local chosen
  for _, name in ipairs(candidates) do
    if name:sub(1, #C.fx_name) == C.fx_name then chosen = name; break end
  end
  check(chosen ~= nil, C.format .. ' delay discovered by REAPER')
  note('Discovered ' .. chosen)

  reaper.GetSetProjectInfo(0, 'PROJECT_SRATE', C.rate, true)
  reaper.GetSetProjectInfo(0, 'PROJECT_SRATE_USE', 1, true)
  reaper.SetCurrentBPM(0, C.tempo, false)
  local master = reaper.GetMasterTrack(0)
  reaper.SetMediaTrackInfo_Value(master, 'D_VOL', 1)
  for send = 0, reaper.GetTrackNumSends(master, 1) - 1 do
    reaper.SetTrackSendInfo_Value(master, 1, send, 'D_VOL', 0) -- silence hardware outputs only
  end
  reaper.InsertTrackAtIndex(0, true)
  track = reaper.GetTrack(0, 0)
  reaper.GetSetMediaTrackInfo_String(track, 'P_NAME', 'RAVET Delay check', true)
  reaper.SetOnlyTrackSelected(track)
  reaper.SetEditCurPos(0, false, false)
  check(reaper.InsertMedia(root .. '/input.wav', 0) == 1, 'synthetic impulse source inserted')
  local item = reaper.GetTrackMediaItem(track, 0)
  check(item ~= nil, 'source item present')
  reaper.SetMediaItemInfo_Value(item, 'D_POSITION', 0)
  reaper.SetMediaItemInfo_Value(item, 'D_FADEINLEN', 0)
  reaper.SetMediaItemInfo_Value(item, 'D_FADEOUTLEN', 0)

  fx = reaper.TrackFX_AddByName(track, chosen, false, -1)
  check(fx >= 0, C.format .. ' delay instantiated on the track')
  local _, instance = reaper.TrackFX_GetFXName(track, fx, '')
  note('Instance ' .. instance)
  locate()

  local fixed, recalled, synced = C.cases[1], C.cases[2], C.cases[3]
  apply(values_of(fixed), 'host edit')
  render(fixed)
  save('delay-fixed')

  apply({time = 500, mix = 35, sync = 1, division = 3}, 'scratch edit before reopen')
  reopen('delay-fixed')
  for _, key in ipairs(ORDER) do expect(key, values_of(fixed)[key], 'saved project recall') end
  render(recalled)

  reaper.SetCurrentBPM(0, C.tempo, false)
  apply(values_of(synced), 'tempo-synced host edit')
  render(synced)
  save('delay-synced')
  reopen('delay-synced')
  for _, key in ipairs(ORDER) do expect(key, values_of(synced)[key], 'synced project recall') end
  check(near(reaper.Master_GetTempo(), C.tempo, 1e-6), 'project tempo recalled')
  save('delay-synced')

  note('PASS REAPER delay discovery/edit/render/recall checks completed')
end

local function finish(ok, err)
  if not ok then note('FAIL ' .. tostring(err)) end
  note(ok and 'DONE ok' or 'DONE failed')
  log:close()
  if C.quit then
    -- Quit on the next main-loop tick so this script has returned first.
    reaper.defer(function() reaper.Main_OnCommand(40004, 0) end)
  end
end

-- Run the check as a coroutine resumed from reaper.defer, so waits keep
-- REAPER's main loop (and plug-in parameter flushing) running.
local task = coroutine.create(function() return xpcall(run, debug.traceback) end)
local function step()
  local resumed, ok, err = coroutine.resume(task)
  if not resumed then finish(false, ok)
  elseif coroutine.status(task) == 'dead' then finish(ok, err)
  else reaper.defer(step) end
end
step()
