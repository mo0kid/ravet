-- Run only in a disposable, empty REAPER project. Uses public REAPER APIs.
-- Fixture and output directory: <REAPER resource directory>/ravet-validation.
local root = reaper.GetResourcePath() .. '/ravet-validation'
local log = assert(io.open(root .. '/host-validation.log', 'w'))
local function note(message) log:write(message .. '\n'); log:flush() end
local function check(condition, message) assert(condition, message); note('PASS ' .. message) end
local function guard(fn)
  local ok, err = xpcall(fn, debug.traceback)
  if not ok then note('FAIL ' .. err); reaper.OnStopButton(); log:close() end
  return ok
end
local track, fx, gain, bypass, deadline, phase
local function value(index) return reaper.TrackFX_GetParam(track, fx, index) end
local function near(a,b) return math.abs(a-b) < 0.0001 end
local function peaks()
  local left,right=reaper.Track_GetPeakInfo(track,0),reaper.Track_GetPeakInfo(track,1)
  note('Live peaks left=' .. left .. ' right=' .. right)
  return left,right
end
local function configure_render(name)
  reaper.GetSetProjectInfo(0,'RENDER_SETTINGS',2,true) -- selected track stems; master remains silent
  reaper.GetSetProjectInfo(0,'RENDER_BOUNDSFLAG',0,true)
  reaper.GetSetProjectInfo(0,'RENDER_STARTPOS',0,true)
  reaper.GetSetProjectInfo(0,'RENDER_ENDPOS',4,true)
  reaper.GetSetProjectInfo(0,'RENDER_CHANNELS',2,true)
  reaper.GetSetProjectInfo(0,'RENDER_SRATE',48000,true)
  reaper.GetSetProjectInfo(0,'RENDER_TAILFLAG',0,true)
  reaper.GetSetProjectInfo(0,'RENDER_DITHER',0,true)
  reaper.GetSetProjectInfo_String(0,'RENDER_FORMAT','evaw',true)
  reaper.GetSetProjectInfo_String(0,'RENDER_FILE',root,true)
  reaper.GetSetProjectInfo_String(0,'RENDER_PATTERN',name,true)
end
local function save(name) reaper.Main_SaveProjectEx(0,root .. '/' .. name .. '.rpp',8) end
local function tick()
  if not guard(function()
    if reaper.time_precise() < deadline then reaper.defer(tick); return end
    if phase == 1 then
      check(near(value(gain),1.5) and near(value(bypass),0),'host parameter updates reach plugin')
      check(reaper.TrackFX_GetFloatingWindow(track,fx) ~= nil,'host owns opened plugin editor window')
      reaper.OnStopButton()
      configure_render('wet'); save('wet')
      reaper.TrackFX_Show(track,fx,2)
      reaper.TrackFX_Show(track,fx,3)
      check(reaper.TrackFX_GetFloatingWindow(track,fx) ~= nil,'editor reopens in REAPER')
      reaper.TrackFX_SetParam(track,fx,gain,0.5)
      reaper.TrackFX_SetParam(track,fx,bypass,1)
      deadline=reaper.time_precise()+1; phase=2; reaper.defer(tick)
    elseif phase == 2 then
      check(near(value(gain),0.5) and near(value(bypass),1),'gain and plugin bypass update together')
      configure_render('dry'); save('dry')
      reaper.TrackFX_SetParam(track,fx,gain,3)
      reaper.TrackFX_SetParam(track,fx,bypass,0)
      reaper.Main_openProject('noprompt:' .. root .. '/dry.rpp')
      track=reaper.GetTrack(0,0); fx=0
      deadline=reaper.time_precise()+1; phase=3; reaper.defer(tick)
    elseif phase == 3 then
      check(near(value(gain),0.5) and near(value(bypass),1),'saved session reload restores gain and plugin bypass')
      reaper.Main_openProject('noprompt:' .. root .. '/wet.rpp')
      track=reaper.GetTrack(0,0); fx=0
      deadline=reaper.time_precise()+1; phase=4; reaper.defer(tick)
    elseif phase == 4 then
      check(near(value(gain),1.5) and near(value(bypass),0),'second saved session restores wet gain')
      local env=reaper.GetFXEnvelope(track,fx,gain,true)
      check(env ~= nil,'gain automation envelope created')
      reaper.DeleteEnvelopePointRange(env,-1,10)
      -- FX envelope values use physical range for this CLAP parameter.
      reaper.InsertEnvelopePoint(env,0,0.5,1,0,false,true)
      reaper.InsertEnvelopePoint(env,2,2.0,1,0,false,true)
      reaper.Envelope_SortPoints(env)
      reaper.SetTrackAutomationMode(track,1)
      configure_render('automation'); save('automation')
      reaper.TrackFX_Show(track,fx,3)
      reaper.SetEditCurPos(0,false,false)
      reaper.OnPlayButton()
      deadline=reaper.time_precise()+1; phase=5; reaper.defer(tick)
    elseif phase == 5 then
      note('Playback first position=' .. reaper.GetPlayPosition() .. ' gain=' .. value(gain))
      check(near(value(gain),0.5),'DAW playback reads first automation value')
      local left,right=peaks()
      check(near(left,0.125) and near(right,0.25),'live stereo audio follows first automation gain')
      reaper.SetEditCurPos(2.5,true,true)
      deadline=reaper.time_precise()+0.25; phase=6; reaper.defer(tick)
    else
      note('Playback second position=' .. reaper.GetPlayPosition() .. ' gain=' .. value(gain))
      check(near(value(gain),2.0),'DAW playback reads second automation value')
      local left,right=peaks()
      check(near(left,0.5) and near(right,1.0),'live stereo audio follows second automation gain')
      reaper.TrackFX_Show(track,fx,2)
      reaper.OnStopButton()
      note('PASS REAPER API lifecycle/state/automation checks completed')
      note('Visual resize, GUI drag and meter checks require native UI inspection.')
      log:close()
    end
  end) then return end
end
guard(function()
  note('REAPER ' .. reaper.GetAppVersion())
  note('Platform ' .. reaper.GetOS())
  check(reaper.CountTracks(0)==0,'disposable empty project')
  reaper.InsertTrackAtIndex(0,true); track=reaper.GetTrack(0,0)
  reaper.GetSetMediaTrackInfo_String(track,'P_NAME','RAVET Validation',true)
  reaper.SetOnlyTrackSelected(track)
  reaper.InsertMedia(root .. '/input.wav',0)
  reaper.SetMediaTrackInfo_Value(reaper.GetMasterTrack(0),'D_VOL',0) -- silent hardware output
  fx=reaper.TrackFX_AddByName(track,'CLAP: RAVET Gain',false,-1)
  check(fx>=0,'CLAP discovered and instantiated')
  local _, name=reaper.TrackFX_GetFXName(track,fx,''); note('Plugin ' .. name)
  for i=0,reaper.TrackFX_GetNumParams(track,fx)-1 do
    local _,name=reaper.TrackFX_GetParamName(track,fx,i,'')
    note('Parameter ' .. i .. ' ' .. name)
    local _, ident=reaper.TrackFX_GetParamIdent(track,fx,i)
    note('Identifier ' .. i .. ' ' .. ident)
    if name=='Gain' and ident:match(':1$') then gain=i elseif name=='Bypass' and ident:match(':2$') then bypass=i end
  end
  check(gain~=nil and bypass~=nil,'gain and bypass parameter metadata')
  check(reaper.TrackFX_SetParam(track,fx,gain,1.5),'gain host edit accepted')
  check(reaper.TrackFX_SetParam(track,fx,bypass,0),'bypass host edit accepted')
  reaper.TrackFX_Show(track,fx,3)
  reaper.SetEditCurPos(0,false,false); reaper.OnPlayButton()
  deadline=reaper.time_precise()+1; phase=1; reaper.defer(tick)
end)
