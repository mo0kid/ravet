-- Run only in an empty disposable resource/project. Generated MIDI, hardware silent.
local root=reaper.GetResourcePath() .. '/ravet-validation'
local log=assert(io.open(root .. '/sine-host.log','w'))
local function note(s) log:write(s .. '\n'); log:flush() end
local function check(ok,s) assert(ok,s); note('PASS ' .. s) end
local track,fx,level,phase,deadline
local function guard(fn)
  local ok,err=xpcall(fn,debug.traceback)
  if not ok then reaper.OnStopButton(); note('FAIL ' .. err); log:close() end
end
local function play()
  reaper.SetEditCurPos(0,false,false); reaper.GetSet_LoopTimeRange(true,false,0,4,false); reaper.GetSetRepeat(1); reaper.OnPlayButton()
end
local function peak(expected)
  local l,r=reaper.Track_GetPeakInfo(track,0),reaper.Track_GetPeakInfo(track,1)
  note('position=' .. reaper.GetPlayPosition() .. ' state=' .. reaper.GetPlayState() .. ' stereo peaks=' .. l .. ',' .. r)
  check(math.abs(l-expected)<.0003 and math.abs(r-expected)<.0003,'actual MIDI instrument stereo amplitude')
end
local function tick()
  guard(function()
    if reaper.time_precise()<deadline then reaper.defer(tick); return end
    local position=reaper.GetPlayPosition()
    local low,high=.6,1.2
    if phase==4 then low,high=1.7,2.1 end
    if position<low or position>high then
      check(reaper.time_precise()<deadline+10,'bounded wait for advancing host transport/held note')
      reaper.defer(tick); return
    end
    if phase==1 then
      check(math.abs(reaper.TrackFX_GetParamNormalized(track,fx,level)-.8)<.00001,'level reaches host')
      peak(.04); check(reaper.TrackFX_GetFloatingWindow(track,fx)~=nil,'generic instrument parameter window')
      reaper.OnStopButton(); reaper.Main_SaveProjectEx(0,root .. '/sine-state.rpp',8)
      reaper.TrackFX_SetParamNormalized(track,fx,level,.2)
      reaper.Main_openProject('noprompt:' .. root .. '/sine-state.rpp'); track=reaper.GetTrack(0,0); fx=0
      check(math.abs(reaper.TrackFX_GetParamNormalized(track,fx,level)-.8)<.00001,'saved instrument level recall')
      play(); phase=2; deadline=reaper.time_precise()+1
    elseif phase==2 then
      peak(.04); reaper.OnStopButton()
      local envelope=reaper.GetFXEnvelope(track,fx,level,true); check(envelope~=nil,'instrument level automation envelope')
      reaper.DeleteEnvelopePointRange(envelope,-1,10)
      reaper.InsertEnvelopePoint(envelope,0,.4,1,0,false,true)
      reaper.InsertEnvelopePoint(envelope,1.5,.8,1,0,false,true)
      reaper.Envelope_SortPoints(envelope); reaper.SetTrackAutomationMode(track,1)
      reaper.Main_SaveProjectEx(0,root .. '/sine-automation.rpp',8)
      play(); phase=3; deadline=reaper.time_precise()+1
    elseif phase==3 then
      check(math.abs(reaper.TrackFX_GetParamNormalized(track,fx,level)-.4)<.00001,'first automated level')
      peak(.02); phase=4; deadline=reaper.time_precise()+.8
    else
      check(math.abs(reaper.TrackFX_GetParamNormalized(track,fx,level)-.8)<.00001,'second automated level')
      peak(.04); reaper.OnStopButton(); reaper.TrackFX_Show(track,fx,2)
      check(reaper.GetPlayState()==0,'test transport stopped')
      local master=reaper.GetMasterTrack(0)
      for i=0,reaper.GetTrackNumSends(master,1)-1 do check(reaper.GetTrackSendInfo_Value(master,1,i,'D_VOL')==0,'hardware send silent') end
      note('PASS REAPER MIDI instrument discovery/audio/state/automation checks completed'); log:close(); return
    end
    reaper.defer(tick)
  end)
end
guard(function()
  note('REAPER ' .. reaper.GetAppVersion() .. ' ' .. reaper.GetOS())
  if reaper.CountTracks(0)==1 then
    local prior=reaper.GetTrack(0,0)
    local _,name=reaper.GetSetMediaTrackInfo_String(prior,'P_NAME','',false)
    check(name=='Public synthetic sine MIDI test','rerun owns only its named synthetic track')
    reaper.OnStopButton(); reaper.DeleteTrack(prior)
  end
  check(reaper.CountTracks(0)==0,'empty disposable test project')
  local name
  for i=0,100000 do local found,display=reaper.EnumInstalledFX(i); if not found then break end
    if display:find('RAVET Sine',1,true) and display:find('CLAP',1,true) then name=display; break end
  end
  check(name~=nil,'sine CLAP discovered in explicit test path'); note(name)
  local master=reaper.GetMasterTrack(0)
  reaper.SetMediaTrackInfo_Value(master,'D_VOL',1)
  for i=0,reaper.GetTrackNumSends(master,1)-1 do reaper.SetTrackSendInfo_Value(master,1,i,'D_VOL',0) end
  reaper.GetSetProjectInfo(0,'PROJECT_SRATE',48000,true); reaper.GetSetProjectInfo(0,'PROJECT_SRATE_USE',0,true)
  reaper.InsertTrackAtIndex(0,true); track=reaper.GetTrack(0,0)
  reaper.GetSetMediaTrackInfo_String(track,'P_NAME','Public synthetic sine MIDI test',true)
  local item=reaper.CreateNewMIDIItemInProj(track,0,4,false); check(item~=nil,'synthetic MIDI item')
  local take=reaper.GetActiveTake(item)
  local start=reaper.MIDI_GetPPQPosFromProjTime(take,.25); local finish=reaper.MIDI_GetPPQPosFromProjTime(take,2.25)
  check(reaper.MIDI_InsertNote(take,false,false,start,finish,0,69,127,false),'synthetic A4 note')
  fx=reaper.TrackFX_AddByName(track,name,false,-1); check(fx>=0,'instrument instantiated')
  for i=0,reaper.TrackFX_GetNumParams(track,fx)-1 do local _,display=reaper.TrackFX_GetParamName(track,fx,i); if display=='Level' then level=i end end
  check(level~=nil,'stable level found'); reaper.TrackFX_SetParamNormalized(track,fx,level,.8)
  reaper.TrackFX_Show(track,fx,3); play(); phase=1; deadline=reaper.time_precise()+1; reaper.defer(tick)
end)
