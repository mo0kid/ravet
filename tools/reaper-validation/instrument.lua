-- Only launch through instrument.py: the driver provides an isolated -cfgfile.
local C = assert(RAVET_SYNTH)
local log = assert(io.open(C.root .. '/delay-host.log', 'w'))
local function note(s) log:write(s .. '\n'); log:flush() end
local function check(ok, s) if not ok then error(s, 2) end; note('PASS ' .. s) end
local function canonical(s) return s:gsub('\\','/'):gsub('/+$','') end
if canonical(reaper.GetResourcePath()) ~= canonical(C.resource) then
  note('ABORT wrong resource directory ' .. reaper.GetResourcePath()); log:close(); return
end
local track, fx, index
local function locate()
  index = {}
  for i=0,reaper.TrackFX_GetNumParams(track,fx)-1 do
    local _,name = reaper.TrackFX_GetParamName(track,fx,i,'')
    if name == 'CC response' then index.cc = i end
    if name == 'Lookahead' then index.lookahead = i end
  end
  check(index.cc and index.lookahead, 'instrument parameters')
end
local function set(key,value)
  reaper.TrackFX_SetParamNormalized(track,fx,index[key],value)
  local deadline = reaper.time_precise()+10
  while reaper.TrackFX_GetParamNormalized(track,fx,index[key]) ~= value do
    if reaper.time_precise()>=deadline then error('parameter delivery timeout') end
    coroutine.yield()
  end
  -- Let latency restart/notification reach the host before starting a render.
  local until_time = reaper.time_precise()+.3
  while reaper.time_precise()<until_time do coroutine.yield() end
  check(reaper.TrackFX_GetParamNormalized(track,fx,index[key]) == value, key .. '=' .. value)
end
local function render(name, latency)
  reaper.GetSetProjectInfo(0,'RENDER_SETTINGS',0,true)
  reaper.GetSetProjectInfo(0,'RENDER_BOUNDSFLAG',0,true)
  reaper.GetSetProjectInfo(0,'RENDER_STARTPOS',0,true)
  reaper.GetSetProjectInfo(0,'RENDER_ENDPOS',C.length,true)
  reaper.GetSetProjectInfo(0,'RENDER_CHANNELS',2,true)
  reaper.GetSetProjectInfo(0,'RENDER_SRATE',C.rate,true)
  reaper.GetSetProjectInfo(0,'RENDER_TAILFLAG',0,true)
  reaper.GetSetProjectInfo(0,'RENDER_DITHER',0,true)
  reaper.GetSetProjectInfo_String(0,'RENDER_FORMAT','evaw',true)
  reaper.GetSetProjectInfo_String(0,'RENDER_FILE',C.root,true)
  reaper.GetSetProjectInfo_String(0,'RENDER_PATTERN',name,true)
  reaper.Main_OnCommand(42230,0)
  check(reaper.file_exists(C.root .. '/' .. name .. '.wav'),'rendered ' .. name)
  local _,pdc = reaper.TrackFX_GetNamedConfigParm(track,fx,'pdc')
  note('PDC ' .. name .. '=' .. tostring(pdc))
  check(tonumber(pdc)==latency,'reported latency ' .. name .. '=' .. latency)
end
local function run()
  note('REAPER ' .. reaper.GetAppVersion() .. ' ' .. reaper.GetOS() .. ' format ' .. C.format)
  check(reaper.CountTracks(0)==0,'fresh project')
  local chosen
  for i=0,100000 do
    local ok,name = reaper.EnumInstalledFX(i); if not ok then break end
    if name:find('RAVET Evidence Synth',1,true) then
      note('Candidate ' .. name)
      if name:lower():sub(1,#C.format)==C.format then chosen=name end
    end
  end
  check(chosen,'instrument discovered')
  reaper.GetSetProjectInfo(0,'PROJECT_SRATE',C.rate,true)
  reaper.GetSetProjectInfo(0,'PROJECT_SRATE_USE',1,true)
  reaper.SetCurrentBPM(0,120,false)
  local master = reaper.GetMasterTrack(0)
  reaper.SetMediaTrackInfo_Value(master,'D_VOL',1)
  for send=0,reaper.GetTrackNumSends(master,1)-1 do reaper.SetTrackSendInfo_Value(master,1,send,'D_VOL',0) end
  reaper.InsertTrackAtIndex(0,true); track=reaper.GetTrack(0,0)
  reaper.SetOnlyTrackSelected(track)
  reaper.GetSetMediaTrackInfo_String(track,'P_NAME','RAVET instrument evidence',true)
  local item=reaper.CreateNewMIDIItemInProj(track,0,C.length,false)
  check(item,'MIDI item')
  reaper.SetMediaItemInfo_Value(item,'D_FADEINLEN',0); reaper.SetMediaItemInfo_Value(item,'D_FADEOUTLEN',0)
  local take=reaper.GetActiveTake(item)
  local function ppq(seconds) return reaper.MIDI_GetPPQPosFromProjTime(take,seconds) end
  check(reaper.MIDI_InsertNote(take,false,false,ppq(.5),ppq(.6),0,60,127,true),'first MIDI note')
  check(reaper.MIDI_InsertCC(take,false,false,ppq(.9),0xb0,0,1,64),'CC 1 change')
  check(reaper.MIDI_InsertNote(take,false,false,ppq(1),ppq(1.1),0,64,127,true),'second MIDI note')
  reaper.MIDI_Sort(take)
  fx=reaper.TrackFX_AddByName(track,chosen,false,-1); check(fx>=0,'instrument instantiated'); locate()
  set('cc',1); set('lookahead',1); render('pdc',32)
  set('lookahead',0); render('zero',0)
  set('lookahead',1); set('cc',0); render('cc_off',32)
  set('cc',1); set('lookahead',0)
  for pin=0,1 do check(reaper.TrackFX_SetPinMappings(track,fx,1,pin,0,0),'disconnect first instrument audio') end
  local second=reaper.TrackFX_AddByName(track,chosen,false,-1); check(second==1,'second instrument for MIDI output')
  render('echo',0)
  reaper.Main_SaveProjectEx(0,C.root .. '/instrument.rpp',0)
  note('DONE PASS instrument audio/CC/PDC/tail/MIDI echo render requests complete')
end
local co=coroutine.create(run)
local function step()
  local ok,err=coroutine.resume(co)
  if not ok then note('FAIL ' .. tostring(err)); note('DONE FAIL'); log:close(); return end
  if coroutine.status(co)~='dead' then reaper.defer(step); return end
  log:close()
  if C.quit then reaper.Main_OnCommand(40004,0) end
end
step()
