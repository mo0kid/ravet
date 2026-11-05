-- RAVET Delay editor check. Generated and launched by tools/reaper-validation/editor.py
-- inside a disposable REAPER resource directory; do not run it in a normal session.
-- The driver prepends a RAVET_EDITOR table. The script inserts the Delay, opens
-- its editor in a floating FX window, sets a parameter from the host while the
-- editor is open, logs every parameter change (so edits made in the editor by
-- the driver's X11 probe, or by hand, are recorded), closes and reopens the
-- editor, removes the plug-in with its editor open, and quits.
local C = assert(RAVET_EDITOR, 'RAVET_EDITOR configuration missing; run through editor.py')
local root = C.root
local log = assert(io.open(root .. '/delay-host.log', 'w'))
local function canonical(path)
  path = path:gsub('\\', '/'):gsub('/+$', '')
  if reaper.GetOS():find('Win', 1, true) then path = path:lower() end
  return path
end
if canonical(reaper.GetResourcePath()) ~= canonical(C.resource) then
  log:write('ABORT resource directory is ' .. reaper.GetResourcePath() .. ', expected ' .. C.resource .. '\n')
  log:close()
  return
end
local function note(s) log:write(string.format('%7.2f %s\n', reaper.time_precise() - (START or 0), s)); log:flush() end
local function check(ok, s) if not ok then error('check failed: ' .. s, 2) end; note('PASS ' .. s) end
START = reaper.time_precise()

local track, fx
local watched = {}
local function exists(path) local f = io.open(path, 'r'); if f then f:close(); return true end; return false end
local function find(name)
  for i = 0, reaper.TrackFX_GetNumParams(track, fx) - 1 do
    local _, text = reaper.TrackFX_GetParamName(track, fx, i, '')
    if text == name then return i end
  end
end
local function poll()
  for name, entry in pairs(watched) do
    local value = reaper.TrackFX_GetParamNormalized(track, fx, entry.index)
    if entry.value == nil or math.abs(value - entry.value) > 1e-6 then
      local _, text = reaper.TrackFX_GetFormattedParamValue(track, fx, entry.index, '')
      if entry.value ~= nil then note(string.format('CHANGE %s %.6f -> %.6f (%s)', name, entry.value, value, text)) end
      entry.value = value
    end
  end
end
-- Wait (yielding to REAPER) for `seconds`, or until `until_file` exists.
local function hold(seconds, until_file)
  local deadline = reaper.time_precise() + seconds
  while reaper.time_precise() < deadline do
    poll()
    if until_file and exists(until_file) then return true end
    coroutine.yield()
  end
  return false
end

local function run()
  reaper.Main_OnCommand(40296, 0) -- select all tracks
  reaper.Main_OnCommand(40005, 0) -- remove them
  reaper.InsertTrackAtIndex(0, true)
  track = reaper.GetTrack(0, 0)
  reaper.GetSetMediaTrackInfo_String(track, 'P_NAME', 'RAVET Editor', true)
  fx = reaper.TrackFX_AddByName(track, C.fx_name, false, -1)
  check(fx >= 0, 'inserted ' .. C.fx_name)
  local _, name = reaper.TrackFX_GetFXName(track, fx, '')
  note('FX ' .. name)
  for _, key in ipairs({'Time', 'Feedback', 'Mix', 'Ping-pong', 'Sync', 'Division', 'Tone'}) do
    local index = find(key)
    if index then watched[key] = {index = index} end
  end
  poll()
  reaper.TrackFX_Show(track, fx, 3) -- floating editor window
  hold(1.5)
  local window = reaper.TrackFX_GetFloatingWindow(track, fx)
  check(window ~= nil, 'floating editor window open')
  check(reaper.TrackFX_GetOpen(track, fx), 'editor reported open')
  note('OPEN editor')
  hold(C.settle)
  -- Host to editor: the probe's capture after this shows the new value.
  local feedback = watched['Feedback']
  if feedback then
    reaper.TrackFX_SetParamNormalized(track, fx, feedback.index, 0.5)
    note('HOST set Feedback normalized 0.5')
  end
  hold(1.0)
  note('READY for probe')
  local probed = hold(C.seconds, C.probe_done)
  note(probed and 'probe finished' or 'hold finished')
  -- Close and reopen the editor, then remove the plug-in with it open.
  reaper.TrackFX_Show(track, fx, 2)
  hold(1.0)
  check(not reaper.TrackFX_GetOpen(track, fx), 'editor closed')
  reaper.TrackFX_Show(track, fx, 3)
  hold(2.0)
  check(reaper.TrackFX_GetFloatingWindow(track, fx) ~= nil, 'editor reopened')
  note('REOPENED editor')
  if C.reopen_done then hold(C.seconds, C.reopen_done) end
  poll()
  watched = {}
  check(reaper.TrackFX_Delete(track, fx), 'removed the plug-in with its editor open')
  hold(1.0)
  -- Save and reopen, so quitting raises no save prompt.
  reaper.Main_SaveProjectEx(0, root .. '/editor.rpp', 0)
  reaper.Main_openProject('noprompt:' .. root .. '/editor.rpp')
  note('DONE editor check')
  if C.quit then reaper.defer(function() reaper.Main_OnCommand(40004, 0) end) end
end

local routine = coroutine.create(run)
local function step()
  local ok, err = coroutine.resume(routine)
  if not ok then
    note('FAIL ' .. tostring(err))
    reaper.Main_SaveProjectEx(0, root .. '/editor.rpp', 0)
    reaper.Main_openProject('noprompt:' .. root .. '/editor.rpp')
    note('DONE with failure')
    if C.quit then reaper.defer(function() reaper.Main_OnCommand(40004, 0) end) end
    return
  end
  if coroutine.status(routine) ~= 'dead' then reaper.defer(step) end
end
step()
