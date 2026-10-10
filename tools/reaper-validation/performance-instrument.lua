-- Public synthetic offline DAW fixture. Run only in a prepared empty test project.
local root=reaper.GetResourcePath()..'/ravet-validation'
local marker=assert(io.open(root..'/public-fixture.txt','r'),'prepare public fixtures first')
assert(marker:read('*a')=='RAVET public synthetic validation\n','public fixture marker')
marker:close()
assert(reaper.CountTracks(0)==0,'new empty disposable project required')
local log=assert(io.open(root..'/performance-host.log','w'))
local function note(text) log:write(text..'\n');log:flush() end
local function check(value,text) assert(value,text);note('PASS '..text) end
local function guard(fn)
 local okay,error=xpcall(fn,debug.traceback)
 if not okay then reaper.OnStopButton();note('FAIL '..error);log:close() end
end
local track,fx,level,item
guard(function()
 note('REAPER '..reaper.GetAppVersion()..' '..reaper.GetOS())
 reaper.OnStopButton()
 local name
 for i=0,100000 do
  local exists,display=reaper.EnumInstalledFX(i)
  if not exists then break end
  if display:find('RAVET Performance Sine',1,true) and display:find('VST3',1,true) then name=display;break end
 end
 check(name~=nil,'performance VST3 discovered in explicit test path');note(name)
 local master=reaper.GetMasterTrack(0)
 reaper.SetMediaTrackInfo_Value(master,'D_VOL',1)
 for send=0,reaper.GetTrackNumSends(master,1)-1 do
  check(reaper.SetTrackSendInfo_Value(master,1,send,'D_VOL',0),'mute test hardware destination')
 end
 reaper.GetSetProjectInfo(0,'PROJECT_SRATE',48000,true)
 reaper.GetSetProjectInfo(0,'PROJECT_SRATE_USE',1,true)
 check(reaper.GetSetProjectInfo_String(0,'RECORD_PATH',root,true),'explicit synthetic media directory')
 reaper.InsertTrackAtIndex(0,true);track=reaper.GetTrack(0,0)
 reaper.GetSetMediaTrackInfo_String(track,'P_NAME','Public synthetic performance MIDI test',true)
 reaper.SetOnlyTrackSelected(track)
 item=reaper.CreateNewMIDIItemInProj(track,0,4,false);check(item~=nil,'synthetic MIDI item')
 reaper.SetMediaItemInfo_Value(item,'D_FADEINLEN',0);reaper.SetMediaItemInfo_Value(item,'D_FADEOUTLEN',0)
 local take=reaper.GetActiveTake(item)
 local first=reaper.MIDI_GetPPQPosFromProjTime(take,.25)
 local last=reaper.MIDI_GetPPQPosFromProjTime(take,2.25)
 check(reaper.MIDI_InsertNote(take,false,false,first,last,0,69,127,false),'synthetic A4 note')
 fx=reaper.TrackFX_AddByName(track,name,false,-1);check(fx>=0,'instrument instantiated')
 for i=0,reaper.TrackFX_GetNumParams(track,fx)-1 do
  local _,display=reaper.TrackFX_GetParamName(track,fx,i)
  if display=='Level' then level=i end
 end
 check(level~=nil,'stable Level found')
 local section=reaper.SectionFromUniqueID(0)
 check(reaper.kbd_getTextFromCmd(40209,section)=='Item: Apply track/take FX to items','verified stereo apply-FX action')
 local function render(label)
  reaper.TrackFX_SetEnabled(track,fx,true)
  reaper.SetActiveTake(reaper.GetTake(item,0))
  reaper.SetMediaItemSelected(item,true)
  local before=reaper.GetActiveTake(item)
  reaper.Main_OnCommand(40209,0)
  local after=reaper.GetActiveTake(item)
  check(after~=nil and after~=before,'actual DAW rendered '..label)
  local source=reaper.GetMediaItemTake_Source(after)
  note('rendered '..label..'='..reaper.GetMediaSourceFileName(source,''))
  check(reaper.GetMediaSourceNumChannels(source)==2,'stereo rendered '..label)
  reaper.TrackFX_SetEnabled(track,fx,false)
 end
 check(reaper.TrackFX_SetParamNormalized(track,fx,level,.8),'set synthetic Level')
 reaper.Main_SaveProjectEx(0,root..'/performance-state.rpp',8)
 render('level')
 check(reaper.TrackFX_SetParamNormalized(track,fx,level,.2),'edit synthetic Level')
 reaper.Main_openProject('noprompt:'..root..'/performance-state.rpp')
 track=reaper.GetTrack(0,0);fx=0;item=reaper.GetTrackMediaItem(track,0);master=reaper.GetMasterTrack(0)
 check(math.abs(reaper.TrackFX_GetParamNormalized(track,fx,level)-.8)<.00001,'saved Level state recall')
 render('recalled')
 local envelope=reaper.GetFXEnvelope(track,fx,level,true);check(envelope~=nil,'Level automation envelope')
 reaper.DeleteEnvelopePointRange(envelope,-1,10)
 reaper.InsertEnvelopePoint(envelope,0,.4,1,0,false,true)
 reaper.InsertEnvelopePoint(envelope,1.5,.8,1,0,false,true)
 reaper.Envelope_SortPoints(envelope);reaper.SetTrackAutomationMode(track,1)
 render('automation')
 reaper.Main_SaveProjectEx(0,root..'/performance-rendered.rpp',8)
 for send=0,reaper.GetTrackNumSends(master,1)-1 do
  check(reaper.GetTrackSendInfo_Value(master,1,send,'D_VOL')==0,'hardware send silent')
 end
 check(reaper.GetPlayState()==0,'test transport remains stopped')
 note('PASS REAPER VST3 MIDI/state/automation renders prepared; check file samples separately')
 log:close()
end)
