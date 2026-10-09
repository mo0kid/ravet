-- Run only in a disposable empty REAPER resource directory with public input.wav.
-- VST3 normalized API values differ from the CLAP physical-value harness.
local root = reaper.GetResourcePath() .. '/ravet-validation'
local log = assert(io.open(root .. '/vst3-host.log', 'w'))
local function note(s) log:write(s .. '\n'); log:flush() end
local function check(ok,s) assert(ok,s); note('PASS ' .. s) end
local function near(a,b) return math.abs(a-b)<0.0002 end
local function guard(fn)
  local ok,err=xpcall(fn,debug.traceback)
  if not ok then note('FAIL ' .. err); reaper.OnStopButton(); log:close() end
  return ok
end
local track,fx,gain,bypass,phase,deadline
local automationSeeks=0
local function params(g,b)
  note('Actual normalized gain=' .. reaper.TrackFX_GetParamNormalized(track,fx,gain) .. ' bypass=' .. reaper.TrackFX_GetParamNormalized(track,fx,bypass))
  check(near(reaper.TrackFX_GetParamNormalized(track,fx,gain),g) and near(reaper.TrackFX_GetParamNormalized(track,fx,bypass),b),'normalized parameter values gain=' .. g .. ' bypass=' .. b)
end
local function peaks(l,r)
  local a,b=reaper.Track_GetPeakInfo(track,0),reaper.Track_GetPeakInfo(track,1)
  note('Live peaks ' .. a .. ' ' .. b)
  note('Playback position ' .. reaper.GetPlayPosition() .. ' state=' .. reaper.GetPlayState())
  check(near(a,l) and near(b,r),'actual DAW stereo processing')
end
local function save(name) reaper.Main_SaveProjectEx(0,root .. '/' .. name .. '.rpp',8) end
local function play()
  reaper.SetEditCurPos(0,false,false)
  reaper.GetSet_LoopTimeRange(true,false,0,4,false)
  reaper.GetSetRepeat(1)
  reaper.OnPlayButton()
end
local function open(name)
  reaper.OnStopButton(); reaper.Main_openProject('noprompt:' .. root .. '/' .. name .. '.rpp')
  track=reaper.GetTrack(0,0); fx=0; play()
end
local function tick()
  guard(function()
    if reaper.time_precise()<deadline then reaper.defer(tick); return end
    -- Sample automation in the interior of each held region. A delayed UI
    -- defer near a loop edge can observe source/resampler transition samples.
    if phase==5 or phase==6 then
      local position=reaper.GetPlayPosition()
      local low,high,target=.25,1.75,.5
      if phase==6 then low,high,target=2.25,3.75,2.5 end
      if position<low or position>high then
        check(automationSeeks<8,'bounded automation-region seek')
        automationSeeks=automationSeeks+1
        reaper.SetEditCurPos(target,true,true)
        deadline=reaper.time_precise()+.25; reaper.defer(tick); return
      end
    end
    if phase==1 then
      params(.375,0); peaks(.375,.75)
      check(reaper.TrackFX_GetFloatingWindow(track,fx)~=nil,'generic host parameter window opened')
      reaper.OnStopButton(); save('vst3-wet')
      reaper.TrackFX_SetParamNormalized(track,fx,gain,.125); reaper.TrackFX_SetParamNormalized(track,fx,bypass,1)
      play(); phase=2; deadline=reaper.time_precise()+1
    elseif phase==2 then
      params(.125,1); peaks(.25,.5); reaper.OnStopButton(); save('vst3-dry')
      reaper.TrackFX_SetParamNormalized(track,fx,gain,.75); reaper.TrackFX_SetParamNormalized(track,fx,bypass,0)
      open('vst3-dry'); phase=3; deadline=reaper.time_precise()+1
    elseif phase==3 then
      params(.125,1); peaks(.25,.5); note('PASS complete saved bypass session recall')
      reaper.TrackFX_SetParamNormalized(track,fx,bypass,0)
      phase=7; deadline=reaper.time_precise()+1
    elseif phase==7 then
      params(.125,0); peaks(.125,.25); note('PASS restored bypassed edit reaches resumed DSP')
      open('vst3-wet'); phase=4; deadline=reaper.time_precise()+1
    elseif phase==4 then
      params(.375,0); peaks(.375,.75); note('PASS complete saved wet session recall')
      reaper.OnStopButton()
      local env=reaper.GetFXEnvelope(track,fx,gain,true); check(env~=nil,'VST3 gain automation envelope')
      reaper.DeleteEnvelopePointRange(env,-1,10)
      reaper.InsertEnvelopePoint(env,0,.125,1,0,false,true)
      reaper.InsertEnvelopePoint(env,2,.5,1,0,false,true)
      reaper.Envelope_SortPoints(env); reaper.SetTrackAutomationMode(track,1)
      save('vst3-automation'); play(); phase=5; deadline=reaper.time_precise()+1
    elseif phase==5 then
      params(.125,0); peaks(.125,.25)
      reaper.SetEditCurPos(2.5,true,true); phase=6; automationSeeks=0; deadline=reaper.time_precise()+.4
    else
      params(.5,0); peaks(.5,1.)
      reaper.OnStopButton(); reaper.TrackFX_Show(track,fx,2)
      note('PASS VST3 REAPER discovery/audio/state/automation/generic-window checks completed')
      log:close(); return
    end
    reaper.defer(tick)
  end)
end
guard(function()
  note('REAPER ' .. reaper.GetAppVersion() .. ' ' .. reaper.GetOS())
  check(reaper.CountTracks(0)==0,'disposable empty project')
  local name
  for i=0,100000 do
    local exists,display=reaper.EnumInstalledFX(i)
    if not exists then break end
    if display:find('VST3: RAVET Gain',1,true) then name=display; break end
  end
  check(name~=nil,'VST3 discovered from explicit test directory'); note(name)
  local master=reaper.GetMasterTrack(0)
  -- Keep the processing path active while muting only physical destinations.
  -- A zero master fader can let a host suspend inaudible processing/metering.
  reaper.SetMediaTrackInfo_Value(master,'D_VOL',1)
  local sends=reaper.GetTrackNumSends(master,1)
  for send=0,sends-1 do
    check(reaper.SetTrackSendInfo_Value(master,1,send,'D_VOL',0),'mute test hardware destination')
  end
  reaper.InsertTrackAtIndex(0,true); track=reaper.GetTrack(0,0); reaper.SetOnlyTrackSelected(track)
  reaper.InsertMedia(root .. '/input.wav',0)
  fx=reaper.TrackFX_AddByName(track,name,false,1); check(fx>=0,'actual VST3 instance')
  for i=0,reaper.TrackFX_GetNumParams(track,fx)-1 do
    local _,n=reaper.TrackFX_GetParamName(track,fx,i)
    local _,id=reaper.TrackFX_GetParamIdent(track,fx,i)
    note('Parameter ' .. i .. ' name=' .. n .. ' id=' .. id)
    if n=='Gain' then gain=i elseif n=='Bypass' and id~='bypass' then bypass=i end
  end
  check(gain~=nil and bypass~=nil,'actual plug-in gain/bypass parameters')
  reaper.TrackFX_SetParamNormalized(track,fx,gain,.375); reaper.TrackFX_SetParamNormalized(track,fx,bypass,0)
  reaper.TrackFX_Show(track,fx,3); play(); phase=1; deadline=reaper.time_precise()+1; reaper.defer(tick)
end)
